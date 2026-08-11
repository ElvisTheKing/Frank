use std::{cmp::Ordering, fmt};

use image_loader::RegistrationImage;
use viewer_model::NormalizedPoint;

const MIN_IMAGE_SIDE: usize = 32;
const PYRAMID_SCALES: [f64; 4] = [1.0, 0.75, 0.5625, 0.421_875];
const MAX_FEATURES_PER_LEVEL: usize = 220;
const MAX_FEATURES: usize = MAX_FEATURES_PER_LEVEL * PYRAMID_SCALES.len();
const DESCRIPTOR_BITS: usize = 256;
const DESCRIPTOR_MARGIN: usize = 22;
const MAX_DESCRIPTOR_DISTANCE: u32 = 112;
const MATCH_RATIO: f64 = 0.90;
const MAX_RANSAC_MATCHES: usize = 180;
const MIN_RANSAC_INLIERS: usize = 8;
const MIN_INLIER_RATIO: f64 = 0.24;
const MIN_INLIER_SPREAD: f64 = 0.14;
const INLIER_THRESHOLD_PIXELS: f64 = 5.0;
const MIN_MODEL_SCALE: f64 = 0.15;
const MAX_MODEL_SCALE: f64 = 6.0;
const MAX_AUTO_ROTATION_DEGREES: f64 = 2.5;
const MAX_DIAGNOSTIC_MATCHES: usize = 64;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AutoRegistrationEstimate {
    pub(crate) reference_points: [NormalizedPoint; 2],
    pub(crate) target_points: [NormalizedPoint; 2],
    pub(crate) mapping_scale: f64,
    pub(crate) translation: NormalizedPoint,
    pub(crate) confidence: f32,
    pub(crate) median_error_pixels: f64,
    pub(crate) rotation_degrees: f64,
    pub(crate) diagnostics: AutoRegistrationDiagnostics,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct AutoRegistrationDiagnostics {
    pub(crate) reference_features: usize,
    pub(crate) target_features: usize,
    pub(crate) candidate_matches: usize,
    pub(crate) inliers: usize,
    pub(crate) inlier_ratio: Option<f64>,
    pub(crate) confidence: Option<f32>,
    pub(crate) median_error_pixels: Option<f64>,
    pub(crate) matches: Vec<DiagnosticFeatureMatch>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DiagnosticFeatureMatch {
    pub(crate) reference: NormalizedPoint,
    pub(crate) target: NormalizedPoint,
    pub(crate) inlier: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AutoRegistrationFailureReason {
    InvalidImage,
    InsufficientFeatures,
    InsufficientMatches,
    NoStableTransform,
    InsufficientInliers,
    ImplausibleScale,
    ExcessiveRotation,
    ClusteredInliers,
    LowConfidence,
    TransformOutsideImage,
}

impl AutoRegistrationFailureReason {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::InvalidImage => "invalid-image",
            Self::InsufficientFeatures => "insufficient-features",
            Self::InsufficientMatches => "insufficient-matches",
            Self::NoStableTransform => "unstable-transform",
            Self::InsufficientInliers => "insufficient-inliers",
            Self::ImplausibleScale => "implausible-scale",
            Self::ExcessiveRotation => "excessive-rotation",
            Self::ClusteredInliers => "clustered-inliers",
            Self::LowConfidence => "low-confidence",
            Self::TransformOutsideImage => "transform-outside-image",
        }
    }
}

impl fmt::Display for AutoRegistrationFailureReason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidImage => "the comparison preview is invalid or too small",
            Self::InsufficientFeatures => "not enough distinct image features were found",
            Self::InsufficientMatches => "too few reliable feature matches were found",
            Self::NoStableTransform => "the matches do not agree on one stable transform",
            Self::InsufficientInliers => "too few matches support the best transform",
            Self::ImplausibleScale => "the estimated scale is outside the supported range",
            Self::ExcessiveRotation => "the images differ by more than 2.5° rotation",
            Self::ClusteredInliers => "the matching features are confined to too small an area",
            Self::LowConfidence => "the best transform did not meet the confidence threshold",
            Self::TransformOutsideImage => "the estimated overlap lies outside the target image",
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AutoRegistrationFailure {
    pub(crate) reason: AutoRegistrationFailureReason,
    pub(crate) diagnostics: AutoRegistrationDiagnostics,
}

#[derive(Clone, Copy, Debug)]
struct Feature {
    point: NormalizedPoint,
    response: f32,
    descriptor: [u64; DESCRIPTOR_BITS / 64],
}

#[derive(Clone, Copy, Debug)]
struct FeatureMatch {
    reference: usize,
    target: usize,
    distance: u32,
}

#[derive(Clone, Copy, Debug)]
struct SimilarityModel {
    a: f64,
    b: f64,
    translation_x: f64,
    translation_y: f64,
}

impl SimilarityModel {
    fn transform(self, point: [f64; 2]) -> [f64; 2] {
        [
            self.a * point[0] - self.b * point[1] + self.translation_x,
            self.b * point[0] + self.a * point[1] + self.translation_y,
        ]
    }

    fn scale(self) -> f64 {
        self.a.hypot(self.b)
    }

    fn rotation_degrees(self) -> f64 {
        self.b.atan2(self.a).to_degrees()
    }
}

#[derive(Clone, Debug)]
struct GrayImage {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
}

impl GrayImage {
    fn from_registration(image: &RegistrationImage) -> Option<Self> {
        (image.width >= MIN_IMAGE_SIDE
            && image.height >= MIN_IMAGE_SIDE
            && image.pixels.len() == image.width * image.height)
            .then(|| Self {
                width: image.width,
                height: image.height,
                pixels: image.pixels.clone(),
            })
    }

    fn get(&self, x: usize, y: usize) -> u8 {
        self.pixels[y * self.width + x]
    }

    fn sample_bilinear(&self, x: f64, y: f64) -> u8 {
        let x = x.clamp(0.0, (self.width - 1) as f64);
        let y = y.clamp(0.0, (self.height - 1) as f64);
        let x0 = x.floor() as usize;
        let y0 = y.floor() as usize;
        let x1 = (x0 + 1).min(self.width - 1);
        let y1 = (y0 + 1).min(self.height - 1);
        let fraction_x = x - x0 as f64;
        let fraction_y = y - y0 as f64;
        let top = f64::from(self.get(x0, y0)) * (1.0 - fraction_x)
            + f64::from(self.get(x1, y0)) * fraction_x;
        let bottom = f64::from(self.get(x0, y1)) * (1.0 - fraction_x)
            + f64::from(self.get(x1, y1)) * fraction_x;
        (top * (1.0 - fraction_y) + bottom * fraction_y)
            .round()
            .clamp(0.0, 255.0) as u8
    }

    fn resized(&self, scale: f64) -> Self {
        let width = ((self.width as f64 * scale).round() as usize).max(1);
        let height = ((self.height as f64 * scale).round() as usize).max(1);
        let mut pixels = Vec::with_capacity(width * height);
        for y in 0..height {
            for x in 0..width {
                let source_x = (x as f64 + 0.5) * self.width as f64 / width as f64 - 0.5;
                let source_y = (y as f64 + 0.5) * self.height as f64 / height as f64 - 0.5;
                pixels.push(self.sample_bilinear(source_x, source_y));
            }
        }
        Self {
            width,
            height,
            pixels,
        }
    }
}

pub(crate) fn estimate_registration(
    reference: &RegistrationImage,
    target: &RegistrationImage,
) -> Result<AutoRegistrationEstimate, AutoRegistrationFailure> {
    let Some(reference_image) = GrayImage::from_registration(reference) else {
        return Err(registration_failure(
            AutoRegistrationFailureReason::InvalidImage,
            AutoRegistrationDiagnostics::default(),
        ));
    };
    let Some(target_image) = GrayImage::from_registration(target) else {
        return Err(registration_failure(
            AutoRegistrationFailureReason::InvalidImage,
            AutoRegistrationDiagnostics::default(),
        ));
    };
    let reference_features = extract_features(&reference_image);
    let target_features = extract_features(&target_image);
    let mut diagnostics = AutoRegistrationDiagnostics {
        reference_features: reference_features.len(),
        target_features: target_features.len(),
        ..AutoRegistrationDiagnostics::default()
    };
    if reference_features.len() < MIN_RANSAC_INLIERS || target_features.len() < MIN_RANSAC_INLIERS {
        return Err(registration_failure(
            AutoRegistrationFailureReason::InsufficientFeatures,
            diagnostics,
        ));
    }
    let matches = match_features(&reference_features, &target_features);
    diagnostics.candidate_matches = matches.len();
    if matches.len() < MIN_RANSAC_INLIERS {
        diagnostics.matches =
            diagnostic_matches(&matches, &reference_features, &target_features, &[]);
        return Err(registration_failure(
            AutoRegistrationFailureReason::InsufficientMatches,
            diagnostics,
        ));
    }
    let mut ransac_matches = matches.clone();
    ransac_matches.sort_by_key(|feature_match| feature_match.distance);
    ransac_matches.truncate(MAX_RANSAC_MATCHES);
    diagnostics.matches =
        diagnostic_matches(&ransac_matches, &reference_features, &target_features, &[]);

    let reference_dimensions = [reference.width as f64, reference.height as f64];
    let target_dimensions = [target.width as f64, target.height as f64];
    let reference_diagonal = reference_dimensions[0].hypot(reference_dimensions[1]);
    let mut best_model = None;
    let mut best_inliers = Vec::new();
    let mut best_error = f64::INFINITY;
    for first in 0..ransac_matches.len() {
        for second in first + 1..ransac_matches.len() {
            let Some(model) = model_from_pair(
                ransac_matches[first],
                ransac_matches[second],
                &reference_features,
                &target_features,
                reference_dimensions,
                target_dimensions,
                reference_diagonal,
            ) else {
                continue;
            };
            let (inliers, error) = model_inliers(
                model,
                &ransac_matches,
                &reference_features,
                &target_features,
                reference_dimensions,
                target_dimensions,
            );
            if inliers.len() > best_inliers.len()
                || (inliers.len() == best_inliers.len() && error < best_error)
            {
                best_model = Some(model);
                best_inliers = inliers;
                best_error = error;
            }
        }
    }

    diagnostics.inliers = best_inliers.len();
    diagnostics.matches = diagnostic_matches(
        &ransac_matches,
        &reference_features,
        &target_features,
        &best_inliers,
    );
    let Some(mut model) = best_model else {
        return Err(registration_failure(
            AutoRegistrationFailureReason::NoStableTransform,
            diagnostics,
        ));
    };
    if best_inliers.len() < MIN_RANSAC_INLIERS {
        return Err(registration_failure(
            AutoRegistrationFailureReason::InsufficientInliers,
            diagnostics,
        ));
    }
    for _ in 0..2 {
        let Some(fitted_model) = fit_similarity(
            &best_inliers,
            &ransac_matches,
            &reference_features,
            &target_features,
            reference_dimensions,
            target_dimensions,
        ) else {
            return Err(registration_failure(
                AutoRegistrationFailureReason::NoStableTransform,
                diagnostics,
            ));
        };
        model = fitted_model;
        let (inliers, _) = model_inliers(
            model,
            &ransac_matches,
            &reference_features,
            &target_features,
            reference_dimensions,
            target_dimensions,
        );
        best_inliers = inliers;
    }

    let inlier_ratio = best_inliers.len() as f64 / ransac_matches.len() as f64;
    diagnostics.inliers = best_inliers.len();
    diagnostics.inlier_ratio = Some(inlier_ratio);
    diagnostics.matches = diagnostic_matches(
        &ransac_matches,
        &reference_features,
        &target_features,
        &best_inliers,
    );
    if best_inliers.len() < MIN_RANSAC_INLIERS || inlier_ratio < MIN_INLIER_RATIO {
        return Err(registration_failure(
            AutoRegistrationFailureReason::InsufficientInliers,
            diagnostics,
        ));
    }
    let scale = model.scale();
    let rotation_degrees = model.rotation_degrees();
    if !(MIN_MODEL_SCALE..=MAX_MODEL_SCALE).contains(&scale) {
        return Err(registration_failure(
            AutoRegistrationFailureReason::ImplausibleScale,
            diagnostics,
        ));
    }
    if rotation_degrees.abs() > MAX_AUTO_ROTATION_DEGREES {
        return Err(registration_failure(
            AutoRegistrationFailureReason::ExcessiveRotation,
            diagnostics,
        ));
    }
    let spread = inlier_spread(
        &best_inliers,
        &ransac_matches,
        &reference_features,
        reference_dimensions,
    );
    if spread < MIN_INLIER_SPREAD {
        return Err(registration_failure(
            AutoRegistrationFailureReason::ClusteredInliers,
            diagnostics,
        ));
    }
    let mut errors = best_inliers
        .iter()
        .map(|&index| {
            match_error(
                model,
                ransac_matches[index],
                &reference_features,
                &target_features,
                reference_dimensions,
                target_dimensions,
            )
        })
        .collect::<Vec<_>>();
    errors.sort_by(f64::total_cmp);
    let median_error_pixels = errors[errors.len() / 2];
    let mean_descriptor_distance = best_inliers
        .iter()
        .map(|&index| f64::from(ransac_matches[index].distance))
        .sum::<f64>()
        / best_inliers.len() as f64;
    let count_confidence = (best_inliers.len() as f64 / 30.0).clamp(0.0, 1.0);
    let spread_confidence = (spread / 0.45).clamp(0.0, 1.0);
    let residual_confidence = (1.0 - median_error_pixels / INLIER_THRESHOLD_PIXELS).clamp(0.0, 1.0);
    let descriptor_confidence =
        (1.0 - mean_descriptor_distance / f64::from(MAX_DESCRIPTOR_DISTANCE)).clamp(0.0, 1.0);
    let confidence = (inlier_ratio
        * count_confidence.sqrt()
        * spread_confidence.sqrt()
        * residual_confidence.sqrt()
        * descriptor_confidence.sqrt())
    .clamp(0.0, 1.0) as f32;
    diagnostics.confidence = Some(confidence);
    diagnostics.median_error_pixels = Some(median_error_pixels);
    if confidence < 0.16 {
        return Err(registration_failure(
            AutoRegistrationFailureReason::LowConfidence,
            diagnostics,
        ));
    }

    let reference_center = inlier_centroid(
        &best_inliers,
        &ransac_matches,
        &reference_features,
        reference_dimensions,
    );
    let reference_point_one = pixel_to_normalized(reference_center, reference_dimensions);
    let horizontal_offset = reference_dimensions[0] * 0.20;
    let second_x = if reference_center[0] + horizontal_offset <= reference_dimensions[0] * 0.90 {
        reference_center[0] + horizontal_offset
    } else {
        reference_center[0] - horizontal_offset
    };
    let second_pixel = [second_x, reference_center[1]];
    let reference_point_two = pixel_to_normalized(second_pixel, reference_dimensions);
    let target_point_one =
        pixel_to_normalized(model.transform(reference_center), target_dimensions);
    let target_point_two = pixel_to_normalized(model.transform(second_pixel), target_dimensions);
    if !point_is_near_image(target_point_one) || !point_is_near_image(target_point_two) {
        return Err(registration_failure(
            AutoRegistrationFailureReason::TransformOutsideImage,
            diagnostics,
        ));
    }
    let transformed_center =
        model.transform([reference_dimensions[0] * 0.5, reference_dimensions[1] * 0.5]);

    Ok(AutoRegistrationEstimate {
        reference_points: [reference_point_one, reference_point_two],
        target_points: [target_point_one, target_point_two],
        mapping_scale: scale,
        translation: NormalizedPoint {
            x: transformed_center[0] / target_dimensions[0] - 0.5,
            y: transformed_center[1] / target_dimensions[1] - 0.5,
        },
        confidence,
        median_error_pixels,
        rotation_degrees,
        diagnostics,
    })
}

fn registration_failure(
    reason: AutoRegistrationFailureReason,
    diagnostics: AutoRegistrationDiagnostics,
) -> AutoRegistrationFailure {
    AutoRegistrationFailure {
        reason,
        diagnostics,
    }
}

fn diagnostic_matches(
    matches: &[FeatureMatch],
    reference_features: &[Feature],
    target_features: &[Feature],
    inliers: &[usize],
) -> Vec<DiagnosticFeatureMatch> {
    matches
        .iter()
        .take(MAX_DIAGNOSTIC_MATCHES)
        .enumerate()
        .map(|(index, feature_match)| DiagnosticFeatureMatch {
            reference: reference_features[feature_match.reference].point,
            target: target_features[feature_match.target].point,
            inlier: inliers.contains(&index),
        })
        .collect()
}

fn extract_features(image: &GrayImage) -> Vec<Feature> {
    let pattern = brief_pattern();
    let mut features = Vec::new();
    for scale in PYRAMID_SCALES {
        let level = if (scale - 1.0).abs() <= f64::EPSILON {
            image.clone()
        } else {
            image.resized(scale)
        };
        if level.width <= DESCRIPTOR_MARGIN * 2 || level.height <= DESCRIPTOR_MARGIN * 2 {
            continue;
        }
        let corners = detect_corners(&level);
        for (x, y, response) in corners {
            features.push(Feature {
                point: NormalizedPoint {
                    x: (x as f64 + 0.5) / level.width as f64,
                    y: (y as f64 + 0.5) / level.height as f64,
                },
                response,
                // Keep descriptors upright instead of estimating a noisy orientation
                // independently; the similarity consensus handles bounded rotation.
                descriptor: describe(&level, x, y, 0.0, &pattern),
            });
        }
    }
    features.sort_by(|left, right| {
        right
            .response
            .partial_cmp(&left.response)
            .unwrap_or(Ordering::Equal)
    });
    features.truncate(MAX_FEATURES);
    features
}

fn detect_corners(image: &GrayImage) -> Vec<(usize, usize, f32)> {
    let mut selected_candidates = Vec::new();
    for threshold in [24_u8, 18, 13, 9, 6] {
        let mut candidates = Vec::new();
        for y in DESCRIPTOR_MARGIN..image.height - DESCRIPTOR_MARGIN {
            for x in DESCRIPTOR_MARGIN..image.width - DESCRIPTOR_MARGIN {
                if let Some(score) = fast_corner_score(image, x, y, threshold) {
                    candidates.push((x, y, score));
                }
            }
        }
        selected_candidates = candidates;
        if selected_candidates.len() >= 100 {
            break;
        }
    }
    selected_candidates
        .sort_by(|left, right| right.2.partial_cmp(&left.2).unwrap_or(Ordering::Equal));
    let mut corners = Vec::with_capacity(MAX_FEATURES_PER_LEVEL);
    for candidate in selected_candidates {
        let sufficiently_separated = corners.iter().all(|&(x, y, _)| {
            let delta_x = x as isize - candidate.0 as isize;
            let delta_y = y as isize - candidate.1 as isize;
            delta_x * delta_x + delta_y * delta_y >= 36
        });
        if sufficiently_separated {
            corners.push(candidate);
            if corners.len() == MAX_FEATURES_PER_LEVEL {
                break;
            }
        }
    }
    corners
}

fn fast_corner_score(image: &GrayImage, x: usize, y: usize, threshold: u8) -> Option<f32> {
    const CIRCLE: [(isize, isize); 16] = [
        (0, -3),
        (1, -3),
        (2, -2),
        (3, -1),
        (3, 0),
        (3, 1),
        (2, 2),
        (1, 3),
        (0, 3),
        (-1, 3),
        (-2, 2),
        (-3, 1),
        (-3, 0),
        (-3, -1),
        (-2, -2),
        (-1, -3),
    ];
    let center = i16::from(image.get(x, y));
    let threshold = i16::from(threshold);
    let differences = CIRCLE.map(|(delta_x, delta_y)| {
        i16::from(image.get(
            x.saturating_add_signed(delta_x),
            y.saturating_add_signed(delta_y),
        )) - center
    });
    let mut bright_run = 0;
    let mut dark_run = 0;
    let mut best_bright = 0;
    let mut best_dark = 0;
    for difference in differences.iter().chain(differences.iter()).take(24) {
        if *difference > threshold {
            bright_run += 1;
            best_bright = best_bright.max(bright_run);
        } else {
            bright_run = 0;
        }
        if *difference < -threshold {
            dark_run += 1;
            best_dark = best_dark.max(dark_run);
        } else {
            dark_run = 0;
        }
    }
    if best_bright < 9 && best_dark < 9 {
        return None;
    }
    Some(
        differences
            .iter()
            .map(|difference| f32::from(difference.abs()))
            .sum(),
    )
}

fn brief_pattern() -> [([f64; 2], [f64; 2]); DESCRIPTOR_BITS] {
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    std::array::from_fn(|_| {
        let first = random_patch_point(&mut state);
        let mut second = random_patch_point(&mut state);
        while first == second {
            second = random_patch_point(&mut state);
        }
        (first, second)
    })
}

fn random_patch_point(state: &mut u64) -> [f64; 2] {
    loop {
        *state ^= *state >> 12;
        *state ^= *state << 25;
        *state ^= *state >> 27;
        let first = state.wrapping_mul(0x2545_f491_4f6c_dd1d);
        *state ^= *state >> 12;
        *state ^= *state << 25;
        *state ^= *state >> 27;
        let second = state.wrapping_mul(0x2545_f491_4f6c_dd1d);
        let x = (first % 31) as i32 - 15;
        let y = (second % 31) as i32 - 15;
        if x * x + y * y <= 225 {
            return [f64::from(x), f64::from(y)];
        }
    }
}

fn describe(
    image: &GrayImage,
    x: usize,
    y: usize,
    angle: f64,
    pattern: &[([f64; 2], [f64; 2]); DESCRIPTOR_BITS],
) -> [u64; DESCRIPTOR_BITS / 64] {
    let (sin, cos) = angle.sin_cos();
    let rotate = |point: [f64; 2]| {
        [
            x as f64 + cos * point[0] - sin * point[1],
            y as f64 + sin * point[0] + cos * point[1],
        ]
    };
    let mut descriptor = [0_u64; DESCRIPTOR_BITS / 64];
    for (index, &(first, second)) in pattern.iter().enumerate() {
        let first = rotate(first);
        let second = rotate(second);
        if image.sample_bilinear(first[0], first[1]) < image.sample_bilinear(second[0], second[1]) {
            descriptor[index / 64] |= 1_u64 << (index % 64);
        }
    }
    descriptor
}

fn match_features(reference: &[Feature], target: &[Feature]) -> Vec<FeatureMatch> {
    if reference.len() < 2 || target.len() < 2 {
        return Vec::new();
    }
    let forward = reference
        .iter()
        .map(|feature| two_nearest(feature, target))
        .collect::<Vec<_>>();
    let reverse = target
        .iter()
        .map(|feature| two_nearest(feature, reference))
        .collect::<Vec<_>>();
    let mut matches = Vec::new();
    for (reference_index, nearest) in forward.into_iter().enumerate() {
        let Some((target_index, best, second)) = nearest else {
            continue;
        };
        if best > MAX_DESCRIPTOR_DISTANCE || f64::from(best) >= f64::from(second) * MATCH_RATIO {
            continue;
        }
        let Some((reverse_reference, reverse_best, reverse_second)) = reverse[target_index] else {
            continue;
        };
        if !same_feature_location(
            reference[reverse_reference].point,
            reference[reference_index].point,
        ) || reverse_best > MAX_DESCRIPTOR_DISTANCE
            || f64::from(reverse_best) >= f64::from(reverse_second) * MATCH_RATIO
        {
            continue;
        }
        matches.push(FeatureMatch {
            reference: reference_index,
            target: target_index,
            distance: best,
        });
    }
    matches.sort_by_key(|feature_match| feature_match.distance);
    let mut spatially_distinct = Vec::<FeatureMatch>::with_capacity(matches.len());
    for feature_match in matches {
        let duplicate = spatially_distinct.iter().any(|selected| {
            same_feature_location(
                reference[selected.reference].point,
                reference[feature_match.reference].point,
            ) || same_feature_location(
                target[selected.target].point,
                target[feature_match.target].point,
            )
        });
        if !duplicate {
            spatially_distinct.push(feature_match);
        }
    }
    spatially_distinct
}

fn two_nearest(query: &Feature, candidates: &[Feature]) -> Option<(usize, u32, u32)> {
    let mut best_index = 0;
    let mut best = u32::MAX;
    for (index, candidate) in candidates.iter().enumerate() {
        let distance = hamming_distance(query.descriptor, candidate.descriptor);
        if distance < best {
            best = distance;
            best_index = index;
        }
    }
    let second = candidates
        .iter()
        .enumerate()
        .filter(|(index, candidate)| {
            *index != best_index
                && !same_feature_location(candidate.point, candidates[best_index].point)
        })
        .map(|(_, candidate)| hamming_distance(query.descriptor, candidate.descriptor))
        .min()
        .unwrap_or(u32::MAX);
    (second < u32::MAX).then_some((best_index, best, second))
}

fn same_feature_location(left: NormalizedPoint, right: NormalizedPoint) -> bool {
    let delta_x = left.x - right.x;
    let delta_y = left.y - right.y;
    delta_x * delta_x + delta_y * delta_y < 0.0001
}

fn hamming_distance(left: [u64; DESCRIPTOR_BITS / 64], right: [u64; DESCRIPTOR_BITS / 64]) -> u32 {
    left.into_iter()
        .zip(right)
        .map(|(left, right)| (left ^ right).count_ones())
        .sum()
}

fn feature_pixel(feature: Feature, dimensions: [f64; 2]) -> [f64; 2] {
    [
        feature.point.x * dimensions[0],
        feature.point.y * dimensions[1],
    ]
}

fn model_from_pair(
    first: FeatureMatch,
    second: FeatureMatch,
    reference_features: &[Feature],
    target_features: &[Feature],
    reference_dimensions: [f64; 2],
    target_dimensions: [f64; 2],
    reference_diagonal: f64,
) -> Option<SimilarityModel> {
    let reference_one = feature_pixel(reference_features[first.reference], reference_dimensions);
    let reference_two = feature_pixel(reference_features[second.reference], reference_dimensions);
    let target_one = feature_pixel(target_features[first.target], target_dimensions);
    let target_two = feature_pixel(target_features[second.target], target_dimensions);
    let reference_delta = [
        reference_two[0] - reference_one[0],
        reference_two[1] - reference_one[1],
    ];
    let target_delta = [target_two[0] - target_one[0], target_two[1] - target_one[1]];
    let denominator =
        reference_delta[0] * reference_delta[0] + reference_delta[1] * reference_delta[1];
    if denominator.sqrt() < reference_diagonal * 0.08 {
        return None;
    }
    let a =
        (target_delta[0] * reference_delta[0] + target_delta[1] * reference_delta[1]) / denominator;
    let b =
        (target_delta[1] * reference_delta[0] - target_delta[0] * reference_delta[1]) / denominator;
    let model = SimilarityModel {
        a,
        b,
        translation_x: target_one[0] - a * reference_one[0] + b * reference_one[1],
        translation_y: target_one[1] - b * reference_one[0] - a * reference_one[1],
    };
    (MIN_MODEL_SCALE..=MAX_MODEL_SCALE)
        .contains(&model.scale())
        .then_some(model)
}

fn model_inliers(
    model: SimilarityModel,
    matches: &[FeatureMatch],
    reference_features: &[Feature],
    target_features: &[Feature],
    reference_dimensions: [f64; 2],
    target_dimensions: [f64; 2],
) -> (Vec<usize>, f64) {
    let mut inliers = Vec::new();
    let mut squared_error = 0.0;
    for (index, &feature_match) in matches.iter().enumerate() {
        let error = match_error(
            model,
            feature_match,
            reference_features,
            target_features,
            reference_dimensions,
            target_dimensions,
        );
        if error <= INLIER_THRESHOLD_PIXELS {
            inliers.push(index);
            squared_error += error * error;
        }
    }
    (inliers, squared_error)
}

fn match_error(
    model: SimilarityModel,
    feature_match: FeatureMatch,
    reference_features: &[Feature],
    target_features: &[Feature],
    reference_dimensions: [f64; 2],
    target_dimensions: [f64; 2],
) -> f64 {
    let reference = feature_pixel(
        reference_features[feature_match.reference],
        reference_dimensions,
    );
    let target = feature_pixel(target_features[feature_match.target], target_dimensions);
    let predicted = model.transform(reference);
    (predicted[0] - target[0]).hypot(predicted[1] - target[1])
}

fn fit_similarity(
    inliers: &[usize],
    matches: &[FeatureMatch],
    reference_features: &[Feature],
    target_features: &[Feature],
    reference_dimensions: [f64; 2],
    target_dimensions: [f64; 2],
) -> Option<SimilarityModel> {
    let count = inliers.len() as f64;
    if count < 2.0 {
        return None;
    }
    let mut reference_centroid = [0.0; 2];
    let mut target_centroid = [0.0; 2];
    for &index in inliers {
        let feature_match = matches[index];
        let reference = feature_pixel(
            reference_features[feature_match.reference],
            reference_dimensions,
        );
        let target = feature_pixel(target_features[feature_match.target], target_dimensions);
        reference_centroid[0] += reference[0];
        reference_centroid[1] += reference[1];
        target_centroid[0] += target[0];
        target_centroid[1] += target[1];
    }
    reference_centroid[0] /= count;
    reference_centroid[1] /= count;
    target_centroid[0] /= count;
    target_centroid[1] /= count;

    let mut denominator = 0.0;
    let mut real = 0.0;
    let mut imaginary = 0.0;
    for &index in inliers {
        let feature_match = matches[index];
        let reference = feature_pixel(
            reference_features[feature_match.reference],
            reference_dimensions,
        );
        let target = feature_pixel(target_features[feature_match.target], target_dimensions);
        let reference_x = reference[0] - reference_centroid[0];
        let reference_y = reference[1] - reference_centroid[1];
        let target_x = target[0] - target_centroid[0];
        let target_y = target[1] - target_centroid[1];
        denominator += reference_x * reference_x + reference_y * reference_y;
        real += target_x * reference_x + target_y * reference_y;
        imaginary += target_y * reference_x - target_x * reference_y;
    }
    if denominator <= f64::EPSILON {
        return None;
    }
    let a = real / denominator;
    let b = imaginary / denominator;
    Some(SimilarityModel {
        a,
        b,
        translation_x: target_centroid[0] - a * reference_centroid[0] + b * reference_centroid[1],
        translation_y: target_centroid[1] - b * reference_centroid[0] - a * reference_centroid[1],
    })
}

fn inlier_spread(
    inliers: &[usize],
    matches: &[FeatureMatch],
    reference_features: &[Feature],
    reference_dimensions: [f64; 2],
) -> f64 {
    let mut minimum = [f64::INFINITY; 2];
    let mut maximum = [f64::NEG_INFINITY; 2];
    for &index in inliers {
        let point = feature_pixel(
            reference_features[matches[index].reference],
            reference_dimensions,
        );
        minimum[0] = minimum[0].min(point[0]);
        minimum[1] = minimum[1].min(point[1]);
        maximum[0] = maximum[0].max(point[0]);
        maximum[1] = maximum[1].max(point[1]);
    }
    (maximum[0] - minimum[0]).hypot(maximum[1] - minimum[1])
        / reference_dimensions[0].hypot(reference_dimensions[1])
}

fn inlier_centroid(
    inliers: &[usize],
    matches: &[FeatureMatch],
    reference_features: &[Feature],
    reference_dimensions: [f64; 2],
) -> [f64; 2] {
    let mut centroid = [0.0; 2];
    for &index in inliers {
        let point = feature_pixel(
            reference_features[matches[index].reference],
            reference_dimensions,
        );
        centroid[0] += point[0];
        centroid[1] += point[1];
    }
    centroid[0] /= inliers.len() as f64;
    centroid[1] /= inliers.len() as f64;
    centroid
}

fn pixel_to_normalized(point: [f64; 2], dimensions: [f64; 2]) -> NormalizedPoint {
    NormalizedPoint {
        x: point[0] / dimensions[0],
        y: point[1] / dimensions[1],
    }
}

fn point_is_near_image(point: NormalizedPoint) -> bool {
    (-0.25..=1.25).contains(&point.x) && (-0.25..=1.25).contains(&point.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pattern(x: f64, y: f64, seed: u32) -> u8 {
        let checker = (((x * 13.0).floor() as i32 + (y * 11.0).floor() as i32) & 1) as f64;
        let rings = (((x - 0.31).hypot(y - 0.38) * 45.0).sin() * 0.5 + 0.5) * 55.0;
        let diagonal = if (y - (0.18 + x * 0.47)).abs() < 0.018 {
            90.0
        } else {
            0.0
        };
        let boxes = if (0.58..0.78).contains(&x) && (0.22..0.47).contains(&y) {
            75.0
        } else {
            0.0
        };
        let hash = ((x * 97.0).floor() as u32).wrapping_mul(73_856_093)
            ^ ((y * 89.0).floor() as u32).wrapping_mul(19_349_663)
            ^ seed.wrapping_mul(83_492_791);
        (25.0 + checker * 45.0 + rings + diagonal + boxes + f64::from(hash & 31))
            .round()
            .clamp(0.0, 255.0) as u8
    }

    fn transformed_image(
        mapping_scale: f64,
        translation_x: f64,
        translation_y: f64,
        exposure: f64,
        seed: u32,
    ) -> RegistrationImage {
        let width = 640;
        let height = 480;
        let pixels = (0..height)
            .flat_map(|y| {
                (0..width).map(move |x| {
                    let target_x = (x as f64 + 0.5) / width as f64;
                    let target_y = (y as f64 + 0.5) / height as f64;
                    let reference_x = 0.5 + (target_x - 0.5 - translation_x) / mapping_scale;
                    let reference_y = 0.5 + (target_y - 0.5 - translation_y) / mapping_scale;
                    (f64::from(pattern(reference_x, reference_y, seed)) * exposure)
                        .round()
                        .clamp(0.0, 255.0) as u8
                })
            })
            .collect();
        RegistrationImage {
            width,
            height,
            pixels,
        }
    }

    fn unrelated_image(seed: u32) -> RegistrationImage {
        let width = 640;
        let height = 480;
        let pixels = (0..height)
            .flat_map(|y| {
                (0..width).map(move |x| {
                    let mut value = (x as u32)
                        .wrapping_mul(0x9e37_79b9)
                        .rotate_left((y % 31) as u32)
                        ^ (y as u32).wrapping_mul(0x85eb_ca6b)
                        ^ seed.wrapping_mul(0xc2b2_ae35);
                    value ^= value >> 16;
                    value = value.wrapping_mul(0x7feb_352d);
                    value ^= value >> 15;
                    (value & 0xff) as u8
                })
            })
            .collect();
        RegistrationImage {
            width,
            height,
            pixels,
        }
    }

    #[derive(Debug)]
    struct TransformFixture {
        name: String,
        seed: u32,
        scale: f64,
        translation_x: f64,
        translation_y: f64,
        exposure: f64,
        scale_tolerance: f64,
        translation_tolerance: f64,
        min_confidence: f32,
    }

    fn transform_fixtures_v1() -> Vec<TransformFixture> {
        include_str!("../tests/fixtures/registration/v1/transforms.tsv")
            .lines()
            .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
            .map(|line| {
                let fields = line.split('\t').collect::<Vec<_>>();
                assert_eq!(fields.len(), 9, "invalid registration fixture: {line}");
                TransformFixture {
                    name: fields[0].to_owned(),
                    seed: fields[1].parse().expect("fixture seed"),
                    scale: fields[2].parse().expect("fixture scale"),
                    translation_x: fields[3].parse().expect("fixture translation x"),
                    translation_y: fields[4].parse().expect("fixture translation y"),
                    exposure: fields[5].parse().expect("fixture exposure"),
                    scale_tolerance: fields[6].parse().expect("fixture scale tolerance"),
                    translation_tolerance: fields[7]
                        .parse()
                        .expect("fixture translation tolerance"),
                    min_confidence: fields[8].parse().expect("fixture minimum confidence"),
                }
            })
            .collect()
    }

    #[test]
    fn versioned_transform_corpus_recovers_expected_alignment() {
        let fixtures = transform_fixtures_v1();
        assert!(
            fixtures.len() >= 5,
            "the v1 corpus should remain meaningful"
        );
        for fixture in fixtures {
            let reference = transformed_image(1.0, 0.0, 0.0, 1.0, fixture.seed);
            let target = transformed_image(
                fixture.scale,
                fixture.translation_x,
                fixture.translation_y,
                fixture.exposure,
                fixture.seed,
            );
            let estimate = estimate_registration(&reference, &target).unwrap_or_else(|failure| {
                panic!(
                    "fixture {} failed [{}]: {} with {:?}",
                    fixture.name,
                    failure.reason.code(),
                    failure.reason,
                    failure.diagnostics
                )
            });

            assert!(
                (estimate.mapping_scale - fixture.scale).abs() <= fixture.scale_tolerance,
                "{} scale: expected {}, got {}",
                fixture.name,
                fixture.scale,
                estimate.mapping_scale
            );
            assert!(
                (estimate.translation.x - fixture.translation_x).abs()
                    <= fixture.translation_tolerance,
                "{} translation x: expected {}, got {}",
                fixture.name,
                fixture.translation_x,
                estimate.translation.x
            );
            assert!(
                (estimate.translation.y - fixture.translation_y).abs()
                    <= fixture.translation_tolerance,
                "{} translation y: expected {}, got {}",
                fixture.name,
                fixture.translation_y,
                estimate.translation.y
            );
            assert!(
                estimate.confidence >= fixture.min_confidence,
                "{} confidence: expected at least {}, got {}",
                fixture.name,
                fixture.min_confidence,
                estimate.confidence
            );
        }
    }

    #[test]
    fn automatic_registration_recovers_translation_scale_and_exposure_change() {
        let reference = transformed_image(1.0, 0.0, 0.0, 1.0, 7);
        let target = transformed_image(1.18, 0.10, -0.07, 0.55, 7);
        let estimate = estimate_registration(&reference, &target).expect("pattern registers");

        assert!((estimate.mapping_scale - 1.18).abs() < 0.04);
        assert!((estimate.translation.x - 0.10).abs() < 0.025);
        assert!((estimate.translation.y + 0.07).abs() < 0.025);
        assert!(estimate.diagnostics.inliers >= MIN_RANSAC_INLIERS);
        assert!(estimate.confidence > 0.2);
        assert!(estimate.rotation_degrees.abs() < 1.0);
    }

    #[test]
    fn automatic_registration_handles_a_large_focal_length_change() {
        let reference = transformed_image(1.0, 0.0, 0.0, 1.0, 11);
        let target = transformed_image(0.43, 0.03, 0.14, 0.72, 11);
        let estimate = estimate_registration(&reference, &target).expect("pattern registers");

        assert!((estimate.mapping_scale - 0.43).abs() < 0.04);
        assert!((estimate.translation.x - 0.03).abs() < 0.025);
        assert!((estimate.translation.y - 0.14).abs() < 0.025);
        assert!(estimate.diagnostics.inliers >= MIN_RANSAC_INLIERS);
        assert!(estimate.median_error_pixels < 2.0);
    }

    #[test]
    fn flat_images_do_not_produce_false_registration() {
        let image = RegistrationImage {
            width: 640,
            height: 480,
            pixels: vec![80; 640 * 480],
        };
        assert_eq!(
            estimate_registration(&image, &image)
                .expect_err("flat images must be rejected")
                .reason,
            AutoRegistrationFailureReason::InsufficientFeatures
        );
    }

    #[test]
    fn unrelated_images_do_not_produce_false_registration() {
        let reference = transformed_image(1.0, 0.0, 0.0, 1.0, 1);
        let unrelated = unrelated_image(99);
        assert!(extract_features(&GrayImage::from_registration(&unrelated).unwrap()).len() > 100);
        let failure =
            estimate_registration(&reference, &unrelated).expect_err("unrelated images must fail");
        assert!(matches!(
            failure.reason,
            AutoRegistrationFailureReason::InsufficientMatches
                | AutoRegistrationFailureReason::NoStableTransform
                | AutoRegistrationFailureReason::InsufficientInliers
                | AutoRegistrationFailureReason::LowConfidence
        ));
        assert!(failure.diagnostics.reference_features >= MIN_RANSAC_INLIERS);
        assert!(failure.diagnostics.target_features >= MIN_RANSAC_INLIERS);
    }

    #[test]
    fn invalid_registration_image_is_rejected() {
        let image = RegistrationImage {
            width: 640,
            height: 480,
            pixels: vec![0; 10],
        };
        assert_eq!(
            estimate_registration(&image, &image)
                .expect_err("invalid storage must fail")
                .reason,
            AutoRegistrationFailureReason::InvalidImage
        );
    }

    #[test]
    #[ignore = "local camera pair is not part of the repository"]
    fn print_local_camera_jpeg_raw_pair_registration() {
        use std::{path::PathBuf, thread, time::Duration};

        let data = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data");
        let paths = [data.join("P7234066.JPG"), data.join("P7234066.ORF")];
        if paths.iter().any(|path| !path.exists()) {
            return;
        }
        let loader = image_loader::ImageLoader::new(2);
        let handles = paths
            .iter()
            .map(|path| loader.load(path))
            .collect::<Vec<_>>();
        let mut decoded = Vec::new();
        while decoded.len() < handles.len() {
            match loader.try_recv() {
                Ok(result) => {
                    let mut image = result.result.expect("camera pair decodes");
                    image.take_reservation();
                    decoded.push(image);
                }
                Err(_) => thread::sleep(Duration::from_millis(10)),
            }
        }
        decoded.sort_by(|left, right| left.path.cmp(&right.path));
        for image in &decoded {
            eprintln!(
                "{}: display={}x{}, source={}x{}, luminance={:?}, rgb={:?}",
                image.path.display(),
                image.width,
                image.height,
                image.source_width,
                image.source_height,
                image.display_linear_luminance_percentiles,
                image.display_linear_rgb_medians,
            );
        }
        eprintln!(
            "pair registration: {:?}",
            estimate_registration(
                &decoded[0].registration_image,
                &decoded[1].registration_image,
            )
        );
    }

    #[test]
    #[ignore = "local diagnostic corpus is not part of the repository"]
    fn print_local_orf_registration_matrix() {
        use std::{
            collections::HashMap,
            fs,
            path::PathBuf,
            thread,
            time::{Duration, Instant},
        };

        let data = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data");
        let paths = fs::read_dir(data)
            .expect("data directory")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("orf"))
            })
            .collect::<Vec<_>>();
        let loader = image_loader::ImageLoader::new(4);
        let handles = paths
            .iter()
            .map(|path| loader.load(path))
            .collect::<Vec<_>>();
        let names_by_request = handles
            .iter()
            .zip(&paths)
            .map(|(handle, path)| {
                (
                    handle.request_id(),
                    path.file_name()
                        .expect("file name")
                        .to_string_lossy()
                        .into_owned(),
                )
            })
            .collect::<HashMap<_, _>>();
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut images = Vec::new();
        while images.len() < paths.len() && Instant::now() < deadline {
            match loader.try_recv() {
                Ok(result) => {
                    let name = names_by_request
                        .get(&result.request_id)
                        .expect("known request")
                        .clone();
                    let decoded = result.result.expect("preview decodes");
                    images.push((name, decoded.registration_image));
                }
                Err(_) => thread::sleep(Duration::from_millis(10)),
            }
        }
        assert_eq!(images.len(), paths.len(), "all previews decoded");
        images.sort_by(|left, right| left.0.cmp(&right.0));
        for (reference_index, (reference_name, reference)) in images.iter().enumerate() {
            for (target_name, target) in images.iter().skip(reference_index + 1) {
                let reference_gray =
                    GrayImage::from_registration(reference).expect("valid reference");
                let target_gray = GrayImage::from_registration(target).expect("valid target");
                let reference_features = extract_features(&reference_gray);
                let target_features = extract_features(&target_gray);
                let matches = match_features(&reference_features, &target_features);
                eprintln!(
                    "{reference_name} -> {target_name}: features {} / {}, matches {}: {:?}",
                    reference_features.len(),
                    target_features.len(),
                    matches.len(),
                    estimate_registration(reference, target)
                );
            }
        }
    }
}
