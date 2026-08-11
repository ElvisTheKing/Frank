use image_loader::LuminanceGrid;

pub(crate) fn exposure_match_ev(reference_median: f32, target_median: f32) -> f32 {
    (reference_median.max(1.0e-6) / target_median.max(1.0e-6))
        .log2()
        .clamp(-4.0, 4.0)
}

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
