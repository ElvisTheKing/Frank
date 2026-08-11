use image_loader::ColorGrid;
#[cfg(test)]
use image_loader::LuminanceGrid;

const VISIBLE_COLOR_SAMPLE_EDGE: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplayTransform {
    pub(crate) exposure_ev: f32,
    pub(crate) gamma: f32,
    pub(crate) color_gain: [f32; 3],
}

pub(crate) struct VisibleImage<'a> {
    pub(crate) grid: &'a ColorGrid,
    pub(crate) pane: &'a viewer_model::Pane,
    pub(crate) area: &'a ui_egui::PanePaintArea,
    pub(crate) transform: DisplayTransform,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct VisibleColorMatch {
    pub(crate) exposure_ev: f32,
    pub(crate) gamma: [f32; 3],
    pub(crate) color_gain: [f32; 3],
    pub(crate) confidence: f32,
    pub(crate) before_error: f32,
    pub(crate) after_error: f32,
    pub(crate) sample_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct VisibleTransform {
    exposure_ev: f32,
    gamma: [f32; 3],
    color_gain: [f32; 3],
}

#[cfg(test)]
pub(crate) fn exposure_match_ev(reference_median: f32, target_median: f32) -> f32 {
    (reference_median.max(1.0e-6) / target_median.max(1.0e-6))
        .log2()
        .clamp(-4.0, 4.0)
}

#[cfg(test)]
pub(crate) fn visible_normalized_region(
    pane: &viewer_model::Pane,
    area: &ui_egui::PanePaintArea,
) -> Option<[f32; 4]> {
    let [image_width, image_height] = pane.image_size?;
    let scale = pane.viewport.source_pixels_per_physical_pixel as f32;
    let (sin, cos) = (pane.alignment_rotation_degrees as f32)
        .to_radians()
        .sin_cos();
    let source_width = cos.abs() * area.physical_size[0] + sin.abs() * area.physical_size[1];
    let source_height = sin.abs() * area.physical_size[0] + cos.abs() * area.physical_size[1];
    let half_width = source_width * scale / image_width.max(1) as f32 * 0.5;
    let half_height = source_height * scale / image_height.max(1) as f32 * 0.5;
    Some([
        (pane.viewport.center.x as f32 - half_width).clamp(0.0, 1.0),
        (pane.viewport.center.y as f32 - half_height).clamp(0.0, 1.0),
        (pane.viewport.center.x as f32 + half_width).clamp(0.0, 1.0),
        (pane.viewport.center.y as f32 + half_height).clamp(0.0, 1.0),
    ])
}

#[cfg(test)]
pub(crate) fn robust_region_luminance(
    grid: &LuminanceGrid,
    region: [f32; 4],
    gamma: f32,
    exposure_ev: f32,
) -> Option<(f32, f32)> {
    if region[2] <= region[0] || region[3] <= region[1] {
        return None;
    }
    let gain = 2.0_f32.powf(exposure_ev);
    let mut values = Vec::new();
    let mut region_cells = 0_usize;
    for y in 0..grid.height {
        let normalized_y = (y as f32 + 0.5) / grid.height as f32;
        if normalized_y < region[1] || normalized_y > region[3] {
            continue;
        }
        for x in 0..grid.width {
            let normalized_x = (x as f32 + 0.5) / grid.width as f32;
            if normalized_x < region[0] || normalized_x > region[2] {
                continue;
            }
            region_cells += 1;
            let value = grid.values[y * grid.width + x].max(0.0).powf(gamma) * gain;
            if (0.002..=0.98).contains(&value) {
                values.push(value);
            }
        }
    }
    if values.len() < 8 {
        return None;
    }
    values.sort_unstable_by(f32::total_cmp);
    let median = values[values.len() / 2];
    let usable_ratio = values.len() as f32 / region_cells.max(1) as f32;
    let sample_confidence = (values.len() as f32 / 64.0).min(1.0);
    Some((median, usable_ratio * sample_confidence))
}

#[cfg(test)]
pub(crate) fn visible_region_luminance(
    grid: &LuminanceGrid,
    pane: &viewer_model::Pane,
    area: &ui_egui::PanePaintArea,
    gamma: f32,
    exposure_ev: f32,
) -> Option<(f32, f32)> {
    robust_region_luminance(
        grid,
        visible_normalized_region(pane, area)?,
        gamma,
        exposure_ev,
    )
}

pub(crate) fn fit_preview_curve(source: [f32; 5], target: [f32; 5]) -> (f32, f32) {
    let pairs = [1_usize, 2, 3].map(|index| {
        (
            source[index].clamp(1.0e-5, 0.999).log2(),
            target[index].clamp(1.0e-5, 0.999).log2(),
        )
    });
    let mean_x = pairs.iter().map(|pair| pair.0).sum::<f32>() / pairs.len() as f32;
    let mean_y = pairs.iter().map(|pair| pair.1).sum::<f32>() / pairs.len() as f32;
    let variance = pairs
        .iter()
        .map(|pair| (pair.0 - mean_x).powi(2))
        .sum::<f32>();
    let covariance = pairs
        .iter()
        .map(|pair| (pair.0 - mean_x) * (pair.1 - mean_y))
        .sum::<f32>();
    let gamma = if variance > 1.0e-6 {
        (covariance / variance).clamp(0.25, 4.0)
    } else {
        1.0
    };
    let exposure_ev = (mean_y - gamma * mean_x).clamp(-6.0, 6.0);
    (exposure_ev, gamma)
}

pub(crate) fn fit_color_gains(source: [f32; 3], target: [f32; 3]) -> [f32; 3] {
    let ratios = std::array::from_fn(|channel| {
        (target[channel].max(1.0e-5) / source[channel].max(1.0e-5)).clamp(1.0 / 16.0, 16.0)
    });
    let neutral = (ratios[0] * ratios[1] * ratios[2]).cbrt().max(1.0e-5);
    ratios.map(|ratio| (ratio / neutral).clamp(0.5, 2.0))
}

fn linear_luminance(rgb: [f32; 3]) -> f32 {
    0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]
}

pub(crate) fn apply_display_transform(rgb: [f32; 3], transform: DisplayTransform) -> [f32; 3] {
    let balanced = std::array::from_fn(|channel| rgb[channel] * transform.color_gain[channel]);
    let luminance = linear_luminance(balanced).max(1.0e-6);
    let mapped = luminance.powf(transform.gamma) * 2.0_f32.powf(transform.exposure_ev);
    balanced.map(|channel| channel * mapped / luminance)
}

fn source_position_for_screen_sample(
    pane: &viewer_model::Pane,
    area: &ui_egui::PanePaintArea,
    screen_x: f32,
    screen_y: f32,
) -> Option<[f32; 2]> {
    let [image_width, image_height] = pane.image_size?;
    let scale = pane.viewport.source_pixels_per_physical_pixel as f32;
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    let screen_dx = (screen_x - 0.5) * area.physical_size[0];
    let screen_dy = (screen_y - 0.5) * area.physical_size[1];
    let (sin, cos) = (-(pane.alignment_rotation_degrees as f32))
        .to_radians()
        .sin_cos();
    let source_dx = (cos * screen_dx - sin * screen_dy) * scale;
    let source_dy = (sin * screen_dx + cos * screen_dy) * scale;
    Some([
        pane.viewport.center.x as f32 + source_dx / image_width.max(1) as f32,
        pane.viewport.center.y as f32 + source_dy / image_height.max(1) as f32,
    ])
}

fn sample_color_grid(grid: &ColorGrid, position: [f32; 2]) -> Option<[f32; 3]> {
    if grid.width == 0
        || grid.height == 0
        || grid.values.len() != grid.width * grid.height
        || !(0.0..=1.0).contains(&position[0])
        || !(0.0..=1.0).contains(&position[1])
    {
        return None;
    }
    let x = (position[0] * grid.width as f32 - 0.5).clamp(0.0, grid.width as f32 - 1.0);
    let y = (position[1] * grid.height as f32 - 0.5).clamp(0.0, grid.height as f32 - 1.0);
    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;
    let x1 = (x0 + 1).min(grid.width - 1);
    let y1 = (y0 + 1).min(grid.height - 1);
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;
    Some(std::array::from_fn(|channel| {
        let top = grid.values[y0 * grid.width + x0][channel] * (1.0 - tx)
            + grid.values[y0 * grid.width + x1][channel] * tx;
        let bottom = grid.values[y1 * grid.width + x0][channel] * (1.0 - tx)
            + grid.values[y1 * grid.width + x1][channel] * tx;
        top * (1.0 - ty) + bottom * ty
    }))
}

fn percentile(values: &mut [f32], fraction: f32) -> f32 {
    values.sort_unstable_by(f32::total_cmp);
    let index = ((values.len().saturating_sub(1)) as f32 * fraction).round() as usize;
    values[index.min(values.len().saturating_sub(1))]
}

fn channel_percentiles(samples: &[[f32; 3]], channel: usize) -> [f32; 5] {
    let values = samples
        .iter()
        .map(|sample| sample[channel])
        .collect::<Vec<_>>();
    [0.01, 0.10, 0.50, 0.90, 0.99].map(|fraction| {
        let mut copy = values.clone();
        percentile(&mut copy, fraction)
    })
}

fn fit_white_preserving_channel_curve(source: [f32; 5], target: [f32; 5]) -> (f32, f32) {
    let mut sum_aa = 0.0;
    let mut sum_ab = 0.0;
    let mut sum_bb = 0.0;
    let mut sum_ay = 0.0;
    let mut sum_by = 0.0;
    for index in 0..source.len() {
        let source = source[index].clamp(1.0e-6, 0.999);
        let target = target[index].clamp(1.0e-6, 0.999);
        let a = source.log2();
        let b = 1.0 - source;
        let y = target.log2();
        sum_aa += a * a;
        sum_ab += a * b;
        sum_bb += b * b;
        sum_ay += a * y;
        sum_by += b * y;
    }
    let determinant = sum_aa * sum_bb - sum_ab * sum_ab;
    if determinant.abs() <= 1.0e-6 {
        return (0.0, 1.0);
    }
    let gamma = ((sum_ay * sum_bb - sum_by * sum_ab) / determinant).clamp(0.25, 4.0);
    let offset_ev = ((sum_by * sum_aa - sum_ay * sum_ab) / determinant).clamp(-6.0, 6.0);
    (offset_ev, gamma)
}

fn color_distribution_error(reference: &[[f32; 3]], target: &[[f32; 3]]) -> f32 {
    const QUANTILES: [f32; 9] = [0.05, 0.10, 0.20, 0.35, 0.50, 0.65, 0.80, 0.90, 0.95];
    let mut total = 0.0;
    let mut comparisons = 0_usize;
    for channel in 0..4 {
        let reference_values = reference
            .iter()
            .map(|sample| {
                if channel == 3 {
                    linear_luminance(*sample)
                } else {
                    sample[channel]
                }
            })
            .collect::<Vec<_>>();
        let target_values = target
            .iter()
            .map(|sample| {
                if channel == 3 {
                    linear_luminance(*sample)
                } else {
                    sample[channel]
                }
            })
            .collect::<Vec<_>>();
        for quantile in QUANTILES {
            let mut reference_copy = reference_values.clone();
            let mut target_copy = target_values.clone();
            total += (percentile(&mut reference_copy, quantile)
                - percentile(&mut target_copy, quantile))
            .abs();
            comparisons += 1;
        }
    }
    total / comparisons.max(1) as f32
}

fn paired_perceptual_error(reference: &[[f32; 3]], target: &[[f32; 3]]) -> f32 {
    let mut total = 0.0;
    let mut comparisons = 0_usize;
    for (reference, target) in reference.iter().zip(target) {
        for channel in 0..3 {
            // The square root is a compact perceptual weighting: a fixed linear-light
            // error in the field matters more than the same error in the bright sky.
            total += (reference[channel].max(0.0).sqrt() - target[channel].max(0.0).sqrt()).abs();
            comparisons += 1;
        }
    }
    total / comparisons.max(1) as f32
}

fn color_match_error(reference: &[[f32; 3]], target: &[[f32; 3]]) -> f32 {
    // Quantiles make the fit tolerant of tiny registration/development differences;
    // paired perceptual error prevents a large bright region from hiding bad shadows.
    color_distribution_error(reference, target) * 0.2
        + paired_perceptual_error(reference, target) * 0.8
}

fn apply_visible_transform(rgb: [f32; 3], transform: VisibleTransform) -> [f32; 3] {
    std::array::from_fn(|channel| {
        let value = rgb[channel].max(1.0e-6);
        let color_offset = transform.color_gain[channel].max(0.25).log2();
        value.powf(transform.gamma[channel])
            * 2.0_f32.powf(transform.exposure_ev + color_offset * (1.0 - value.clamp(0.0, 1.0)))
    })
}

fn transform_from_parameters(parameters: [f32; 6]) -> Option<VisibleTransform> {
    let [
        exposure_ev,
        red_gamma,
        green_gamma,
        blue_gamma,
        red_log2,
        green_log2,
    ] = parameters;
    let blue_log2 = -red_log2 - green_log2;
    if !(-6.0..=6.0).contains(&exposure_ev)
        || [red_gamma, green_gamma, blue_gamma]
            .iter()
            .any(|gamma| !(0.25..=4.0).contains(gamma))
        || !(-2.0..=2.0).contains(&red_log2)
        || !(-2.0..=2.0).contains(&green_log2)
        || !(-2.0..=2.0).contains(&blue_log2)
    {
        return None;
    }
    Some(VisibleTransform {
        exposure_ev,
        gamma: [red_gamma, green_gamma, blue_gamma],
        color_gain: [red_log2, green_log2, blue_log2].map(f32::exp2),
    })
}

fn parameters_from_transform(transform: VisibleTransform) -> [f32; 6] {
    [
        transform.exposure_ev,
        transform.gamma[0],
        transform.gamma[1],
        transform.gamma[2],
        transform.color_gain[0].max(0.25).log2(),
        transform.color_gain[1].max(0.25).log2(),
    ]
}

fn transformed_samples(samples: &[[f32; 3]], transform: VisibleTransform) -> Vec<[f32; 3]> {
    samples
        .iter()
        .map(|&sample| apply_visible_transform(sample, transform))
        .collect()
}

fn refine_visible_transform(
    reference: &[[f32; 3]],
    target: &[[f32; 3]],
    initial: VisibleTransform,
) -> (VisibleTransform, f32) {
    let identity = VisibleTransform {
        exposure_ev: 0.0,
        gamma: [1.0; 3],
        color_gain: [1.0; 3],
    };
    let mut best_transform = identity;
    let mut best_error = color_match_error(reference, target);
    let initial_error = color_match_error(reference, &transformed_samples(target, initial));
    if initial_error < best_error {
        best_transform = initial;
        best_error = initial_error;
    }
    let mut parameters = parameters_from_transform(best_transform);
    let mut steps = [1.0_f32, 0.25, 0.25, 0.25, 0.25, 0.25];
    for _ in 0..7 {
        for coordinate in 0..parameters.len() {
            for _ in 0..8 {
                let mut improved = false;
                for direction in [-1.0_f32, 1.0] {
                    let mut candidate_parameters = parameters;
                    candidate_parameters[coordinate] += direction * steps[coordinate];
                    let Some(candidate) = transform_from_parameters(candidate_parameters) else {
                        continue;
                    };
                    let error =
                        color_match_error(reference, &transformed_samples(target, candidate));
                    if error + 1.0e-6 < best_error {
                        parameters = candidate_parameters;
                        best_transform = candidate;
                        best_error = error;
                        improved = true;
                    }
                }
                if !improved {
                    break;
                }
            }
        }
        steps = steps.map(|step| step * 0.5);
    }
    (best_transform, best_error)
}

pub(crate) fn fit_visible_color_match(
    reference: VisibleImage<'_>,
    target: VisibleImage<'_>,
) -> Option<VisibleColorMatch> {
    let mut reference_samples = Vec::with_capacity(VISIBLE_COLOR_SAMPLE_EDGE.pow(2));
    let mut target_samples = Vec::with_capacity(VISIBLE_COLOR_SAMPLE_EDGE.pow(2));
    for y in 0..VISIBLE_COLOR_SAMPLE_EDGE {
        for x in 0..VISIBLE_COLOR_SAMPLE_EDGE {
            let screen_x = (x as f32 + 0.5) / VISIBLE_COLOR_SAMPLE_EDGE as f32;
            let screen_y = (y as f32 + 0.5) / VISIBLE_COLOR_SAMPLE_EDGE as f32;
            let reference = sample_color_grid(
                reference.grid,
                source_position_for_screen_sample(
                    reference.pane,
                    reference.area,
                    screen_x,
                    screen_y,
                )?,
            )
            .map(|rgb| apply_display_transform(rgb, reference.transform));
            let target = sample_color_grid(
                target.grid,
                source_position_for_screen_sample(target.pane, target.area, screen_x, screen_y)?,
            )
            .map(|rgb| apply_display_transform(rgb, target.transform));
            let (Some(reference), Some(target)) = (reference, target) else {
                continue;
            };
            let reference_luminance = linear_luminance(reference);
            let target_luminance = linear_luminance(target);
            if (1.0e-5..=0.995).contains(&reference_luminance)
                && (1.0e-5..=0.995).contains(&target_luminance)
            {
                reference_samples.push(reference);
                target_samples.push(target);
            }
        }
    }
    if reference_samples.len() < 64 {
        return None;
    }

    let channel_fits = std::array::from_fn(|channel| {
        fit_white_preserving_channel_curve(
            channel_percentiles(&target_samples, channel),
            channel_percentiles(&reference_samples, channel),
        )
    });
    let exposure_ev = channel_fits.iter().map(|fit| fit.0).sum::<f32>() / 3.0;
    let initial_transform = VisibleTransform {
        exposure_ev,
        gamma: channel_fits.map(|fit| fit.1),
        color_gain: channel_fits.map(|fit| 2.0_f32.powf(fit.0 - exposure_ev)),
    };
    let before_error = color_match_error(&reference_samples, &target_samples);
    let (transform, after_error) =
        refine_visible_transform(&reference_samples, &target_samples, initial_transform);
    let coverage =
        (reference_samples.len() as f32 / VISIBLE_COLOR_SAMPLE_EDGE.pow(2) as f32).clamp(0.0, 1.0);
    let improvement = if before_error <= 1.0e-6 {
        1.0
    } else {
        (1.0 - after_error / before_error).clamp(0.0, 1.0)
    };
    Some(VisibleColorMatch {
        exposure_ev: transform.exposure_ev,
        gamma: transform.gamma,
        color_gain: transform.color_gain,
        confidence: coverage * improvement,
        before_error,
        after_error,
        sample_count: reference_samples.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposure_match_uses_linear_ev_ratio_and_is_bounded() {
        assert!((exposure_match_ev(0.2, 0.1) - 1.0).abs() < 1.0e-6);
        assert!((exposure_match_ev(0.1, 0.2) + 1.0).abs() < 1.0e-6);
        assert_eq!(exposure_match_ev(1.0, 1.0e-9), 4.0);
    }

    #[test]
    fn preview_curve_recovers_known_power_and_gain() {
        let source = [0.005_f32, 0.02, 0.07, 0.22, 0.45];
        let expected_gamma = 0.7_f32;
        let expected_ev = 0.8_f32;
        let target = source.map(|value| value.powf(expected_gamma) * 2.0_f32.powf(expected_ev));
        let (ev, gamma) = fit_preview_curve(source, target);
        assert!((ev - expected_ev).abs() < 1.0e-4);
        assert!((gamma - expected_gamma).abs() < 1.0e-4);
    }

    #[test]
    fn preview_curve_uses_neutral_gamma_for_flat_source_samples() {
        let (ev, gamma) = fit_preview_curve([0.1; 5], [0.4; 5]);
        assert!((ev - 2.0).abs() < 1.0e-5);
        assert_eq!(gamma, 1.0);
    }

    #[test]
    fn color_match_separates_chroma_from_overall_exposure() {
        let gains = fit_color_gains([0.10, 0.20, 0.40], [0.24, 0.32, 1.20]);
        assert!((gains[0] * gains[1] * gains[2] - 1.0).abs() < 1.0e-5);
        assert!((gains[0] / gains[1] - 1.5).abs() < 1.0e-5);
        assert!((gains[2] / gains[1] - 1.875).abs() < 1.0e-5);
    }

    #[test]
    fn visible_color_match_recovers_a_known_tone_and_color_transform() {
        let width = 16;
        let height = 16;
        let target_values = (0..height)
            .flat_map(|y| {
                (0..width).map(move |x| {
                    let x = x as f32 / (width - 1) as f32;
                    let y = y as f32 / (height - 1) as f32;
                    [
                        0.03 + 0.22 * x,
                        0.05 + 0.30 * y,
                        0.04 + 0.18 * (x + y) * 0.5,
                    ]
                })
            })
            .collect::<Vec<_>>();
        let expected = VisibleTransform {
            exposure_ev: 0.7,
            gamma: [0.75, 0.9, 1.1],
            color_gain: [1.2, 1.0, 1.0 / 1.2],
        };
        let reference_values = target_values
            .iter()
            .map(|&rgb| apply_visible_transform(rgb, expected))
            .collect::<Vec<_>>();
        let target_grid = ColorGrid {
            width,
            height,
            values: target_values,
        };
        let reference_grid = ColorGrid {
            width,
            height,
            values: reference_values,
        };
        let mut reference_pane = viewer_model::Pane::placeholder(1, "reference");
        reference_pane.image_size = Some([width as u32, height as u32]);
        reference_pane.viewport.source_pixels_per_physical_pixel = 1.0;
        let mut target_pane = viewer_model::Pane::placeholder(2, "target");
        target_pane.image_size = Some([width as u32, height as u32]);
        target_pane.viewport.source_pixels_per_physical_pixel = 1.0;
        let reference_area = ui_egui::PanePaintArea {
            pane_id: reference_pane.id,
            rect: egui::Rect::NOTHING,
            physical_size: [width as f32, height as f32],
        };
        let target_area = ui_egui::PanePaintArea {
            pane_id: target_pane.id,
            ..reference_area
        };
        let identity = DisplayTransform {
            exposure_ev: 0.0,
            gamma: 1.0,
            color_gain: [1.0; 3],
        };

        let fitted = fit_visible_color_match(
            VisibleImage {
                grid: &reference_grid,
                pane: &reference_pane,
                area: &reference_area,
                transform: identity,
            },
            VisibleImage {
                grid: &target_grid,
                pane: &target_pane,
                area: &target_area,
                transform: identity,
            },
        )
        .expect("the full visible grids provide enough samples");

        let recovered = transformed_samples(
            &target_grid.values,
            VisibleTransform {
                exposure_ev: fitted.exposure_ev,
                gamma: fitted.gamma,
                color_gain: fitted.color_gain,
            },
        );
        let recovered_error = color_match_error(&reference_grid.values, &recovered);
        assert!(
            recovered_error < 0.02,
            "recovered error {recovered_error}, fit {fitted:?}"
        );
        assert!(fitted.after_error < fitted.before_error * 0.15);
        assert!(fitted.confidence > 0.75);
    }

    #[test]
    fn visible_color_match_fades_chroma_correction_at_white() {
        let transform = VisibleTransform {
            exposure_ev: 0.25,
            gamma: [1.0; 3],
            color_gain: [1.4, 0.8, 1.0 / (1.4 * 0.8)],
        };
        let white = apply_visible_transform([1.0; 3], transform);
        assert!((white[0] - white[1]).abs() < 1.0e-6);
        assert!((white[1] - white[2]).abs() < 1.0e-6);
    }

    #[test]
    #[ignore = "local camera pair is not part of the repository"]
    fn print_local_camera_pair_visible_color_match() {
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
        let estimate = crate::registration::estimate_registration(
            &decoded[0].registration_image,
            &decoded[1].registration_image,
        )
        .expect("camera pair registers");
        let mut workspace = viewer_model::Workspace::demo();
        workspace.panes.truncate(2);
        workspace.reference_pane = Some(workspace.panes[0].id);
        workspace
            .set_image(
                workspace.panes[0].id,
                viewer_model::ImageId(1),
                [decoded[0].source_width, decoded[0].source_height],
                "JPEG",
                viewer_model::ImageMetadata::default(),
            )
            .unwrap();
        workspace
            .set_image(
                workspace.panes[1].id,
                viewer_model::ImageId(2),
                [decoded[1].source_width, decoded[1].source_height],
                "ORF preview",
                viewer_model::ImageMetadata::default(),
            )
            .unwrap();
        let physical_size = [720.0, 810.0];
        for index in 0..2 {
            let pane_id = workspace.panes[index].id;
            let [width, height] = workspace.panes[index].image_size.unwrap();
            let fit_scale = (width as f64 / physical_size[0] as f64)
                .max(height as f64 / physical_size[1] as f64);
            workspace.update_pane_fit_scale(pane_id, fit_scale);
        }
        workspace
            .align_pane_from_points(
                workspace.panes[0].id,
                workspace.panes[1].id,
                estimate.reference_points,
                estimate.target_points,
            )
            .unwrap();
        let reference_area = ui_egui::PanePaintArea {
            pane_id: workspace.panes[0].id,
            rect: egui::Rect::NOTHING,
            physical_size,
        };
        let target_area = ui_egui::PanePaintArea {
            pane_id: workspace.panes[1].id,
            ..reference_area
        };
        let identity = DisplayTransform {
            exposure_ev: 0.0,
            gamma: 1.0,
            color_gain: [1.0; 3],
        };
        let color_match = fit_visible_color_match(
            VisibleImage {
                grid: &decoded[0].color_grid,
                pane: &workspace.panes[0],
                area: &reference_area,
                transform: identity,
            },
            VisibleImage {
                grid: &decoded[1].color_grid,
                pane: &workspace.panes[1],
                area: &target_area,
                transform: identity,
            },
        )
        .expect("the visible registered camera pair provides enough samples");
        eprintln!("camera pair visible match: {color_match:?}");
        assert!(color_match.after_error < color_match.before_error);
    }

    #[test]
    fn region_luminance_rejects_clipped_extremes() {
        let grid = LuminanceGrid {
            width: 4,
            height: 4,
            values: vec![
                0.0, 0.1, 0.2, 1.0, 0.0, 0.1, 0.2, 1.0, 0.0, 0.1, 0.2, 1.0, 0.0, 0.1, 0.2, 1.0,
            ],
        };
        let (median, confidence) = robust_region_luminance(&grid, [0.0, 0.0, 1.0, 1.0], 1.0, 0.0)
            .expect("eight mid-range samples remain");
        assert!((median - 0.2).abs() < 1.0e-6);
        assert!(confidence > 0.0 && confidence < 1.0);
    }

    #[test]
    fn visible_region_uses_viewport_scale_and_clamps_to_image_edges() {
        let mut pane = viewer_model::Pane::placeholder(1, "candidate");
        let area = ui_egui::PanePaintArea {
            pane_id: pane.id,
            rect: egui::Rect::NOTHING,
            physical_size: [200.0, 100.0],
        };
        assert_eq!(visible_normalized_region(&pane, &area), None);

        pane.image_size = Some([1_000, 500]);
        pane.viewport.center = viewer_model::NormalizedPoint { x: 0.1, y: 0.9 };
        let region = visible_normalized_region(&pane, &area).expect("image has dimensions");
        for (actual, expected) in region.into_iter().zip([0.0, 0.8, 0.2, 1.0]) {
            assert!((actual - expected).abs() < 1.0e-6);
        }
    }

    #[test]
    fn registered_panes_sample_their_own_visible_image_regions() {
        let grid = LuminanceGrid {
            width: 20,
            height: 10,
            values: (0..10)
                .flat_map(|_| (0..20).map(|x| if x < 10 { 0.1 } else { 0.4 }))
                .collect(),
        };
        let area = ui_egui::PanePaintArea {
            pane_id: viewer_model::PaneId(1),
            rect: egui::Rect::NOTHING,
            physical_size: [400.0, 200.0],
        };
        let mut reference = viewer_model::Pane::placeholder(1, "reference");
        reference.image_size = Some([1_000, 500]);
        reference.viewport.center = viewer_model::NormalizedPoint { x: 0.25, y: 0.5 };
        let mut target = viewer_model::Pane::placeholder(2, "target");
        target.image_size = Some([1_000, 500]);
        target.viewport.center = viewer_model::NormalizedPoint { x: 0.75, y: 0.5 };

        let reference_sample =
            visible_region_luminance(&grid, &reference, &area, 1.0, 0.0).unwrap();
        let target_sample = visible_region_luminance(&grid, &target, &area, 1.0, 0.0).unwrap();

        assert!((reference_sample.0 - 0.1).abs() < 1.0e-6);
        assert!((target_sample.0 - 0.4).abs() < 1.0e-6);
    }

    #[test]
    fn region_luminance_rejects_empty_or_undersampled_regions() {
        let grid = LuminanceGrid {
            width: 2,
            height: 2,
            values: vec![0.1; 4],
        };
        assert_eq!(
            robust_region_luminance(&grid, [0.5, 0.5, 0.5, 0.9], 1.0, 0.0),
            None
        );
        assert_eq!(
            robust_region_luminance(&grid, [0.0, 0.0, 1.0, 1.0], 1.0, 0.0),
            None
        );
    }
}
