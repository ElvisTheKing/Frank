use image_loader::{DecodeQuality, RawDevelopOptions, RawRecipe};

#[cfg(test)]
use image_loader::RawDisplayMode;

pub(crate) fn preview_detail_exhausted(
    source_pixels_per_physical_pixel: f64,
    preview_size: [u32; 2],
    source_size: [u32; 2],
) -> bool {
    let preview_limit = (f64::from(source_size[0]) / f64::from(preview_size[0].max(1)))
        .max(f64::from(source_size[1]) / f64::from(preview_size[1].max(1)));
    source_pixels_per_physical_pixel < preview_limit
}

pub(crate) fn raw_options_match(left: RawDevelopOptions, right: RawDevelopOptions) -> bool {
    left.mode == right.mode && (left.comparison_match_ev - right.comparison_match_ev).abs() < 0.005
}

pub(crate) fn raw_recipe_matches(recipe: &RawRecipe, options: RawDevelopOptions) -> bool {
    recipe.display_mode == options.mode
        && (recipe.comparison_match_ev - options.comparison_match_ev).abs() < 0.005
}

pub(crate) fn full_raw_satisfies_resolution_request(
    full_raw_pending: bool,
    quality: Option<DecodeQuality>,
) -> bool {
    full_raw_pending || quality == Some(DecodeQuality::FullRaw)
}

pub(crate) fn needs_full_raw_development(
    is_raw_source: bool,
    full_raw_pending: bool,
    quality: Option<DecodeQuality>,
) -> bool {
    is_raw_source && !full_raw_satisfies_resolution_request(full_raw_pending, quality)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_raw_is_requested_only_after_preview_native_resolution() {
        let preview = [3_200, 2_400];
        let source = [10_368, 7_776];
        let native_preview_limit = 3.24;

        assert!(!preview_detail_exhausted(
            native_preview_limit,
            preview,
            source
        ));
        assert!(preview_detail_exhausted(3.0, preview, source));
    }

    #[test]
    fn completed_or_pending_full_raw_satisfies_resolution_requests() {
        assert!(!full_raw_satisfies_resolution_request(false, None));
        assert!(full_raw_satisfies_resolution_request(true, None));
        assert!(full_raw_satisfies_resolution_request(
            false,
            Some(DecodeQuality::FullRaw)
        ));
    }

    #[test]
    fn develop_all_selects_only_unfinished_raw_sources() {
        assert!(needs_full_raw_development(
            true,
            false,
            Some(DecodeQuality::EmbeddedPreview)
        ));
        assert!(!needs_full_raw_development(
            false,
            false,
            Some(DecodeQuality::Full)
        ));
        assert!(!needs_full_raw_development(
            true,
            true,
            Some(DecodeQuality::EmbeddedPreview)
        ));
        assert!(!needs_full_raw_development(
            true,
            false,
            Some(DecodeQuality::FullRaw)
        ));
    }

    #[test]
    fn raw_recipe_deduplication_is_recipe_aware() {
        let options = RawDevelopOptions {
            mode: RawDisplayMode::Reference,
            comparison_match_ev: 0.0,
        };
        assert!(raw_options_match(options, options));

        let mut recipe = RawRecipe {
            display_mode: RawDisplayMode::Reference,
            ..RawRecipe::default()
        };
        assert!(raw_recipe_matches(&recipe, options));
        recipe.display_mode = RawDisplayMode::AsShot;
        assert!(!raw_recipe_matches(&recipe, options));
    }

    #[test]
    fn raw_recipe_matching_tolerates_only_substep_ev_noise() {
        let base = RawDevelopOptions {
            mode: RawDisplayMode::Reference,
            comparison_match_ev: 1.0,
        };
        let within_tolerance = RawDevelopOptions {
            comparison_match_ev: 1.004,
            ..base
        };
        let outside_tolerance = RawDevelopOptions {
            comparison_match_ev: 1.01,
            ..base
        };
        assert!(raw_options_match(base, within_tolerance));
        assert!(!raw_options_match(base, outside_tolerance));

        let recipe = RawRecipe {
            display_mode: RawDisplayMode::Reference,
            comparison_match_ev: 1.0,
            ..RawRecipe::default()
        };
        assert!(raw_recipe_matches(&recipe, within_tolerance));
        assert!(!raw_recipe_matches(&recipe, outside_tolerance));
    }
}
