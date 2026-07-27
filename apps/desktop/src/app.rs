use std::{collections::HashMap, path::PathBuf, time::Duration};

use eframe::egui::{self, Align2, Color32, FontId, pos2};
use image_loader::{
    DecodeQuality, DecodeReservation, ImageLoader, RawDevelopOptions, RawDisplayMode,
};
use renderer_wgpu::{PaneRenderState, TileRenderer, UploadImage, UploadTile};
use rfd::FileDialog;
use ui_egui::{RawModeChoice, UiState};
use viewer_model::{ImageId, ImageMetadata, MAX_PANES, PaneId, Workspace};

use crate::{
    comparison::{
        exposure_match_ev, fit_preview_curve, intersect_region, robust_region_luminance,
        visible_normalized_region,
    },
    pane_runtime::{PaneRuntime, PaneStatus, file_display_name},
    preferences::{PREFERENCES_KEY, PersistedPreferences},
    raw_pipeline::{
        full_raw_satisfies_resolution_request, preview_detail_exhausted, raw_options_match,
        raw_recipe_matches, selected_raw_mode,
    },
    workspace_batch::resize_workspace_for_batch,
};

const APP_NAME: &str = "ImageCompareTool";
const APP_ID: &str = "org.imagecomparetool.desktop";
const GPU_UPLOAD_BUDGET_PER_FRAME: usize = 32 * 1024 * 1024;

pub fn run() -> eframe::Result {
    let native_options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default()
            .with_app_id(APP_ID)
            .with_title(APP_NAME)
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([900.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        APP_NAME,
        native_options,
        Box::new(|creation_context| Ok(Box::new(DesktopApp::new(creation_context)))),
    )
}

struct DesktopApp {
    workspace: Workspace,
    ui_state: UiState,
    pane_runtime: HashMap<PaneId, PaneRuntime>,
    loader: ImageLoader,
    renderer: TileRenderer,
    next_image_id: u64,
    pending_preview_match: Option<PaneId>,
}

impl DesktopApp {
    fn new(creation_context: &eframe::CreationContext<'_>) -> Self {
        creation_context
            .egui_ctx
            .set_theme(egui::ThemePreference::System);
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = Color32::from_rgb(22, 26, 31);
        visuals.window_fill = Color32::from_rgb(28, 33, 39);
        visuals.extreme_bg_color = Color32::from_rgb(13, 16, 20);
        visuals.faint_bg_color = Color32::from_rgb(34, 40, 47);
        visuals.selection.bg_fill = Color32::from_rgb(50, 105, 150);
        visuals.widgets.inactive.bg_fill = Color32::from_rgb(38, 44, 51);
        visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(34, 40, 47);
        visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, Color32::from_rgb(55, 63, 72));
        visuals.widgets.inactive.fg_stroke =
            egui::Stroke::new(1.0, Color32::from_rgb(207, 214, 221));
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(48, 57, 66);
        visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(47, 56, 65);
        visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, Color32::from_rgb(76, 91, 105));
        visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, Color32::WHITE);
        visuals.widgets.active.bg_fill = Color32::from_rgb(54, 112, 158);
        visuals.widgets.active.weak_bg_fill = Color32::from_rgb(48, 94, 130);
        visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, Color32::from_rgb(88, 156, 208));
        visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, Color32::WHITE);
        visuals.widgets.open = visuals.widgets.active;
        for widget in [
            &mut visuals.widgets.inactive,
            &mut visuals.widgets.hovered,
            &mut visuals.widgets.active,
            &mut visuals.widgets.open,
        ] {
            widget.corner_radius = egui::CornerRadius::same(3);
            widget.expansion = 0.0;
        }
        visuals.window_corner_radius = egui::CornerRadius::same(4);
        creation_context
            .egui_ctx
            .set_visuals_of(egui::Theme::Dark, visuals);

        let mut light_visuals = egui::Visuals::light();
        light_visuals.panel_fill = Color32::from_rgb(239, 242, 245);
        light_visuals.window_fill = Color32::from_rgb(249, 250, 251);
        light_visuals.extreme_bg_color = Color32::from_rgb(218, 223, 228);
        light_visuals.faint_bg_color = Color32::from_rgb(229, 233, 237);
        light_visuals.selection.bg_fill = Color32::from_rgb(54, 121, 171);
        light_visuals.widgets.inactive.bg_fill = Color32::from_rgb(225, 230, 234);
        light_visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(232, 236, 240);
        light_visuals.widgets.inactive.bg_stroke =
            egui::Stroke::new(1.0, Color32::from_rgb(179, 187, 195));
        light_visuals.widgets.inactive.fg_stroke =
            egui::Stroke::new(1.0, Color32::from_rgb(42, 49, 56));
        light_visuals.widgets.hovered.bg_fill = Color32::from_rgb(214, 222, 228);
        light_visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(219, 226, 232);
        light_visuals.widgets.hovered.bg_stroke =
            egui::Stroke::new(1.0, Color32::from_rgb(128, 145, 159));
        light_visuals.widgets.hovered.fg_stroke =
            egui::Stroke::new(1.0, Color32::from_rgb(20, 25, 30));
        light_visuals.widgets.active.bg_fill = Color32::from_rgb(75, 136, 180);
        light_visuals.widgets.active.weak_bg_fill = Color32::from_rgb(83, 143, 185);
        light_visuals.widgets.active.bg_stroke =
            egui::Stroke::new(1.0, Color32::from_rgb(43, 105, 151));
        light_visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, Color32::WHITE);
        light_visuals.widgets.open = light_visuals.widgets.active;
        for widget in [
            &mut light_visuals.widgets.inactive,
            &mut light_visuals.widgets.hovered,
            &mut light_visuals.widgets.active,
            &mut light_visuals.widgets.open,
        ] {
            widget.corner_radius = egui::CornerRadius::same(3);
            widget.expansion = 0.0;
        }
        light_visuals.window_corner_radius = egui::CornerRadius::same(4);
        creation_context
            .egui_ctx
            .set_visuals_of(egui::Theme::Light, light_visuals);
        creation_context.egui_ctx.all_styles_mut(|style| {
            style.spacing.item_spacing = egui::vec2(7.0, 5.0);
            style.spacing.button_padding = egui::vec2(10.0, 5.0);
            style.spacing.interact_size = egui::vec2(36.0, 26.0);
            style.spacing.menu_margin = egui::Margin::same(8);
            style.animation_time = 0.08;
        });
        let render_state = creation_context
            .wgpu_render_state
            .clone()
            .expect("the desktop host is configured to require WGPU");
        let mut workspace = Workspace::demo();
        let mut ui_state = UiState::default();
        if let Some(preferences) = creation_context
            .storage
            .and_then(PersistedPreferences::load)
        {
            preferences.apply(&mut workspace, &mut ui_state);
        }
        let pane_runtime = workspace
            .panes
            .iter()
            .map(|pane| (pane.id, PaneRuntime::default()))
            .collect();
        let worker_count = std::thread::available_parallelism()
            .map_or(2, |parallelism| parallelism.get().saturating_sub(1))
            .clamp(1, 4);
        let startup_paths = std::env::args_os().skip(1).map(PathBuf::from).collect();
        let mut app = Self {
            workspace,
            ui_state,
            pane_runtime,
            loader: ImageLoader::new(worker_count),
            renderer: TileRenderer::new(render_state),
            next_image_id: 1,
            pending_preview_match: None,
        };
        app.open_paths(startup_paths);
        app
    }

    fn open_dialog(&mut self) {
        let extensions = image_loader::supported_extensions();
        if let Some(paths) = FileDialog::new()
            .set_title("Open images to compare")
            .add_filter("Supported images", &extensions)
            .pick_files()
        {
            self.open_paths(paths);
        }
    }

    fn open_dialog_for(&mut self, pane_id: PaneId) {
        let extensions = image_loader::supported_extensions();
        if let Some(path) = FileDialog::new()
            .set_title("Open or replace image")
            .add_filter("Supported images", &extensions)
            .pick_file()
        {
            self.open_path(pane_id, path);
        }
    }

    fn open_paths(&mut self, paths: Vec<PathBuf>) {
        let paths: Vec<_> = paths.into_iter().take(MAX_PANES).collect();
        if paths.is_empty() {
            return;
        }
        let changes = resize_workspace_for_batch(&mut self.workspace, paths.len());
        for pane_id in changes.removed {
            self.remove_pane_runtime(pane_id);
        }
        for pane_id in changes.added {
            self.pane_runtime.insert(pane_id, PaneRuntime::default());
        }
        let targets: Vec<_> = self
            .workspace
            .panes
            .iter()
            .take(paths.len())
            .map(|pane| pane.id)
            .collect();
        for (pane_id, path) in targets.into_iter().zip(paths) {
            self.open_path(pane_id, path);
        }
    }

    fn apply_pane_changes(&mut self, output: &ui_egui::UiOutput) {
        for &pane_id in &output.removed_panes {
            self.remove_pane_runtime(pane_id);
        }
        for &pane_id in &output.added_panes {
            self.pane_runtime.entry(pane_id).or_default();
        }
    }

    fn remove_pane_runtime(&mut self, pane_id: PaneId) {
        if let Some(mut runtime) = self.pane_runtime.remove(&pane_id) {
            runtime.handle.take();
            if let Some(image_id) = runtime.image_id.take() {
                self.renderer.remove_image(image_id);
            }
        }
    }

    fn close_image(&mut self, pane_id: PaneId) {
        self.ui_state.cancel_note_edit_for(pane_id);
        self.remove_pane_runtime(pane_id);
        self.pane_runtime.insert(pane_id, PaneRuntime::default());
        self.workspace
            .clear_image(pane_id, format!("Pane {}", pane_id.0));
    }

    fn open_path(&mut self, pane_id: PaneId, path: PathBuf) {
        self.ui_state.cancel_note_edit_for(pane_id);
        let Some(runtime) = self.pane_runtime.get_mut(&pane_id) else {
            return;
        };
        runtime.handle.take();
        if let Some(image_id) = runtime.image_id.take() {
            self.renderer.remove_image(image_id);
        }
        let display_name = file_display_name(&path);
        self.workspace.clear_image(pane_id, display_name);
        let image_id = ImageId(self.next_image_id);
        self.next_image_id += 1;
        let handle = self.loader.load(path.clone());
        runtime.image_id = Some(image_id);
        runtime.source_path = Some(path.clone());
        runtime.is_raw_source = image_loader::is_raw_path(&path);
        runtime.full_raw_pending = false;
        runtime.pending_raw_options = None;
        runtime.full_raw_error = None;
        runtime.raw_recipe = None;
        runtime.raw_diagnostics = None;
        runtime.display_linear_stats = None;
        runtime.preview_linear_stats = None;
        runtime.luminance_grid = None;
        runtime.display_size = None;
        runtime.source_size = None;
        runtime.quality = None;
        if let Some(pane) = self
            .workspace
            .panes
            .iter_mut()
            .find(|pane| pane.id == pane_id)
        {
            pane.preview_match_ev = 0.0;
            pane.exposure_match_ev = 0.0;
            pane.manual_exposure_ev = 0.0;
            pane.normalization_confidence = None;
        }
        runtime.status = PaneStatus::Decoding { path };
        runtime.handle = Some(handle);
        let _ = self.workspace.set_active(pane_id);
    }

    fn request_full_raw_for_active_pane(&mut self) {
        let Some(pane_id) = self.workspace.active_pane else {
            return;
        };
        let already_full_or_pending = self.pane_runtime.get(&pane_id).is_some_and(|runtime| {
            full_raw_satisfies_resolution_request(runtime.full_raw_pending, runtime.quality)
        });
        if !already_full_or_pending {
            self.request_raw_development(pane_id, selected_raw_mode(self.ui_state.raw_mode), 0.0);
        }
    }

    fn request_raw_development(
        &mut self,
        pane_id: PaneId,
        mode: RawDisplayMode,
        comparison_match_ev: f32,
    ) {
        let Some(runtime) = self.pane_runtime.get_mut(&pane_id) else {
            return;
        };
        if !runtime.is_raw_source {
            return;
        }
        let options = RawDevelopOptions {
            mode,
            comparison_match_ev,
        };
        if runtime
            .pending_raw_options
            .is_some_and(|pending| raw_options_match(pending, options))
            || (runtime.quality == Some(DecodeQuality::FullRaw)
                && runtime
                    .raw_recipe
                    .as_ref()
                    .is_some_and(|recipe| raw_recipe_matches(recipe, options)))
        {
            return;
        }
        let Some(path) = runtime.source_path.clone() else {
            return;
        };
        runtime.handle = Some(self.loader.load_full_raw_with_options(path, options));
        runtime.full_raw_pending = true;
        runtime.pending_raw_options = Some(options);
        runtime.full_raw_error = None;
    }

    fn process_loader_results(&mut self) {
        let mut develop_after_load = Vec::new();
        while let Ok(load_result) = self.loader.try_recv() {
            let pane_id = self.pane_runtime.iter().find_map(|(pane_id, runtime)| {
                runtime
                    .handle
                    .as_ref()
                    .filter(|handle| handle.request_id() == load_result.request_id)
                    .map(|_| *pane_id)
            });
            let Some(pane_id) = pane_id else {
                continue;
            };
            let runtime = self
                .pane_runtime
                .get_mut(&pane_id)
                .expect("pane runtime exists");
            runtime.handle.take();
            let was_full_raw_request = runtime.full_raw_pending;
            let Some(image_id) = runtime.image_id else {
                continue;
            };
            match load_result.result {
                Ok(mut decoded) => {
                    runtime.full_raw_pending = false;
                    runtime.pending_raw_options = None;
                    let path = decoded.path.clone();
                    let decode_time = decoded.decode_time;
                    let image_size = [decoded.width, decoded.height];
                    let source_size = [decoded.source_width, decoded.source_height];
                    let quality = decoded.quality;
                    runtime.display_size = Some(image_size);
                    runtime.source_size = Some(source_size);
                    runtime.quality = Some(quality);
                    let camera = decoded.camera.clone();
                    let lens = decoded.lens.clone();
                    let bit_depth = decoded.bit_depth;
                    let capture = decoded.capture.clone();
                    let raw_recipe = decoded.raw_recipe.clone();
                    runtime.raw_recipe = decoded.raw_recipe.clone();
                    runtime.raw_diagnostics = decoded.raw_diagnostics.clone();
                    runtime.display_linear_stats =
                        Some(decoded.display_linear_luminance_percentiles);
                    runtime.luminance_grid = Some(decoded.luminance_grid.clone());
                    if quality == DecodeQuality::EmbeddedPreview {
                        runtime.preview_linear_stats = runtime.display_linear_stats;
                    }
                    let total_bytes = decoded.byte_len();
                    let reservation = decoded.take_reservation();
                    let upload = UploadImage {
                        image_id,
                        width: decoded.width,
                        height: decoded.height,
                        tiles: decoded
                            .tiles
                            .into_iter()
                            .map(|tile| UploadTile {
                                x: tile.x,
                                y: tile.y,
                                width: tile.width,
                                height: tile.height,
                                rgba: tile.rgba,
                            })
                            .collect(),
                    };
                    self.renderer.enqueue_image(upload);
                    let source_megapixels =
                        f64::from(source_size[0]) * f64::from(source_size[1]) / 1_000_000.0;
                    let title = file_display_name(&path);
                    let title_metadata = ImageMetadata {
                        megapixels: Some(source_megapixels),
                        camera: camera.clone(),
                        lens: lens.clone(),
                        bit_depth,
                        iso: capture.iso,
                        shutter: capture.shutter,
                        aperture: capture.aperture,
                        focal_length: capture.focal_length,
                        quality: match quality {
                            DecodeQuality::EmbeddedPreview => Some("preview".to_owned()),
                            DecodeQuality::FullRaw => raw_recipe.as_ref().map_or_else(
                                || Some("full raw".to_owned()),
                                |recipe| {
                                    Some(format!(
                                        "raw {:?} {:+.2} EV{}",
                                        recipe.display_mode,
                                        recipe.automatic_exposure_ev,
                                        if recipe.comparison_match_ev.abs() >= 0.005 {
                                            format!(
                                                " · match {:+.2} EV",
                                                recipe.comparison_match_ev
                                            )
                                        } else {
                                            String::new()
                                        }
                                    ))
                                },
                            ),
                            DecodeQuality::Full => None,
                        },
                    };
                    let _ = self.workspace.set_image(
                        pane_id,
                        image_id,
                        source_size,
                        title,
                        title_metadata,
                    );
                    runtime.status = PaneStatus::Uploading {
                        decode_time,
                        total_bytes,
                        source_size,
                        quality,
                        bit_depth,
                        reservation,
                    };
                    if quality == DecodeQuality::EmbeddedPreview
                        && self.ui_state.develop_raws_on_load
                    {
                        develop_after_load.push(pane_id);
                    }
                }
                Err(error) => {
                    if was_full_raw_request {
                        runtime.full_raw_pending = false;
                        runtime.pending_raw_options = None;
                        runtime.full_raw_error = Some(error.to_string());
                    } else {
                        runtime.status = PaneStatus::Error {
                            message: error.to_string(),
                        };
                        runtime.image_id = None;
                    }
                }
            }
        }
        for pane_id in develop_after_load {
            self.request_raw_development(pane_id, selected_raw_mode(self.ui_state.raw_mode), 0.0);
        }
    }

    fn request_selected_raw_mode(&mut self, mode: RawModeChoice) {
        let Some(pane_id) = self.workspace.active_pane else {
            return;
        };
        let mode = selected_raw_mode(mode);
        self.request_raw_development(pane_id, mode, 0.0);
    }

    fn request_raws_past_preview_resolution(&mut self) {
        let pane_scales: HashMap<_, _> = self
            .workspace
            .panes
            .iter()
            .map(|pane| (pane.id, pane.viewport.source_pixels_per_physical_pixel))
            .collect();
        let requests: Vec<_> = self
            .pane_runtime
            .iter()
            .filter_map(|(&pane_id, runtime)| {
                if !runtime.is_raw_source
                    || runtime.full_raw_pending
                    || runtime.quality != Some(DecodeQuality::EmbeddedPreview)
                {
                    return None;
                }
                let [preview_width, preview_height] = runtime.display_size?;
                let [source_width, source_height] = runtime.source_size?;
                preview_detail_exhausted(
                    pane_scales.get(&pane_id).copied()?,
                    [preview_width, preview_height],
                    [source_width, source_height],
                )
                .then_some(pane_id)
            })
            .collect();
        let mode = selected_raw_mode(self.ui_state.raw_mode);
        for pane_id in requests {
            self.request_raw_development(pane_id, mode, 0.0);
        }
    }

    fn begin_exposure_match(&mut self, paint_areas: &[ui_egui::PanePaintArea]) {
        let Some(reference) = self.workspace.active_pane else {
            return;
        };
        let Some(reference_pane) = self
            .workspace
            .panes
            .iter()
            .find(|pane| pane.id == reference)
        else {
            return;
        };
        let Some(reference_area) = paint_areas.iter().find(|area| area.pane_id == reference) else {
            return;
        };
        let Some(reference_region) = visible_normalized_region(reference_pane, reference_area)
        else {
            return;
        };
        let mut results = Vec::new();
        for target_pane in &self.workspace.panes {
            let Some(target_area) = paint_areas
                .iter()
                .find(|area| area.pane_id == target_pane.id)
            else {
                continue;
            };
            let Some(target_region) = visible_normalized_region(target_pane, target_area) else {
                continue;
            };
            let region = intersect_region(reference_region, target_region);
            let reference_sample = self.pane_runtime.get(&reference).and_then(|runtime| {
                runtime.luminance_grid.as_ref().and_then(|grid| {
                    robust_region_luminance(
                        grid,
                        region,
                        reference_pane.display_gamma(),
                        reference_pane.preview_match_ev + reference_pane.manual_exposure_ev,
                    )
                })
            });
            let target_sample = self.pane_runtime.get(&target_pane.id).and_then(|runtime| {
                runtime.luminance_grid.as_ref().and_then(|grid| {
                    robust_region_luminance(
                        grid,
                        region,
                        target_pane.display_gamma(),
                        target_pane.preview_match_ev + target_pane.manual_exposure_ev,
                    )
                })
            });
            if let (
                Some((reference_median, reference_confidence)),
                Some((target_median, target_confidence)),
            ) = (reference_sample, target_sample)
            {
                results.push((
                    target_pane.id,
                    exposure_match_ev(reference_median, target_median),
                    reference_confidence.min(target_confidence),
                ));
            }
        }
        for (pane_id, ev, confidence) in results {
            if let Some(pane) = self
                .workspace
                .panes
                .iter_mut()
                .find(|pane| pane.id == pane_id)
            {
                pane.exposure_match_ev = ev;
                pane.normalization_confidence = Some(confidence);
            }
        }
    }

    fn continue_exposure_match(&mut self) {
        let Some(pane_id) = self.pending_preview_match else {
            return;
        };
        let Some(runtime) = self.pane_runtime.get(&pane_id) else {
            self.pending_preview_match = None;
            return;
        };
        if runtime.full_raw_pending {
            return;
        }
        if let (Some(preview), Some(current)) =
            (runtime.preview_linear_stats, runtime.display_linear_stats)
            && let Some(pane) = self
                .workspace
                .panes
                .iter_mut()
                .find(|pane| pane.id == pane_id)
        {
            let (ev, gamma) = fit_preview_curve(current, preview);
            pane.preview_match_ev = ev;
            pane.preview_match_gamma = gamma;
        }
        self.pending_preview_match = None;
    }

    fn match_active_raw_to_preview(&mut self) {
        let Some(pane_id) = self.workspace.active_pane else {
            return;
        };
        let Some(runtime) = self.pane_runtime.get(&pane_id) else {
            return;
        };
        if !runtime.is_raw_source || runtime.preview_linear_stats.is_none() {
            return;
        }
        self.pending_preview_match = Some(pane_id);
        if !matches!(
            runtime.status,
            PaneStatus::Ready {
                quality: DecodeQuality::FullRaw,
                ..
            }
        ) {
            self.request_raw_development(pane_id, RawDisplayMode::AsShot, 0.0);
        }
    }

    fn reset_exposure_match(&mut self) {
        self.pending_preview_match = None;
        for pane in &mut self.workspace.panes {
            pane.exposure_match_ev = 0.0;
            pane.normalization_confidence = None;
            pane.preview_match_ev = 0.0;
            pane.preview_match_gamma = 1.0;
        }
    }

    fn update_uploads(&mut self) {
        self.renderer
            .upload_with_budget(GPU_UPLOAD_BUDGET_PER_FRAME);
        for runtime in self.pane_runtime.values_mut() {
            let PaneStatus::Uploading {
                decode_time,
                total_bytes,
                source_size,
                quality,
                bit_depth,
                reservation,
            } = &runtime.status
            else {
                continue;
            };
            let ready = (
                *decode_time,
                *total_bytes,
                *source_size,
                *quality,
                *bit_depth,
                reservation.as_ref().map(DecodeReservation::reserved_bytes),
            );
            let Some(image_id) = runtime.image_id else {
                continue;
            };
            if self
                .renderer
                .upload_progress(image_id)
                .is_some_and(|progress| progress.is_complete())
            {
                let (decode_time, total_bytes, source_size, quality, bit_depth, _reserved_bytes) =
                    ready;
                runtime.status = PaneStatus::Ready {
                    decode_time,
                    total_bytes,
                    source_size,
                    quality,
                    bit_depth,
                };
            }
        }
    }

    fn add_image_callbacks(&self, ui: &mut egui::Ui, output: &ui_egui::UiOutput) {
        for area in &output.paint_areas {
            let Some(pane) = self
                .workspace
                .panes
                .iter()
                .find(|pane| pane.id == area.pane_id)
            else {
                continue;
            };
            if let Some(image_id) = pane.image_id {
                ui.painter().add(self.renderer.paint_callback(
                    area.rect,
                    PaneRenderState {
                        pane_id: pane.id,
                        image_id,
                        center: [pane.viewport.center.x as f32, pane.viewport.center.y as f32],
                        source_size: pane.image_size.unwrap_or([1, 1]),
                        source_pixels_per_physical_pixel:
                            pane.viewport.source_pixels_per_physical_pixel as f32,
                        physical_size: area.physical_size,
                        exposure_ev: pane.display_exposure_ev(),
                        gamma: pane.display_gamma(),
                    },
                ));
            }
            if let Some(runtime) = self.pane_runtime.get(&pane.id) {
                self.paint_status(ui, area, runtime);
            }
            if !self.ui_state.show_pane_controls && pane.image_id.is_some() {
                let mut label = pane.formatted_title(self.workspace.title_fields);
                if !pane.note.is_empty() {
                    label.push_str("  •  ");
                    label.push_str(&pane.note);
                }
                let clip = egui::Rect::from_min_max(
                    pos2(area.rect.left() + 6.0, area.rect.top() + 4.0),
                    pos2(area.rect.right() - 6.0, area.rect.top() + 24.0),
                );
                ui.painter().with_clip_rect(clip).text(
                    clip.left_center(),
                    Align2::LEFT_CENTER,
                    label,
                    FontId::proportional(10.5),
                    ui.visuals().weak_text_color(),
                );
            }
        }
    }

    fn paint_status(&self, ui: &egui::Ui, area: &ui_egui::PanePaintArea, runtime: &PaneRuntime) {
        let normal_text = ui.visuals().text_color();
        let status = if runtime.full_raw_pending {
            Some(("Developing full RAW…".to_owned(), normal_text))
        } else if let Some(message) = &runtime.full_raw_error {
            Some((
                format!("Full RAW development failed\n{message}"),
                Color32::LIGHT_RED,
            ))
        } else {
            match &runtime.status {
                PaneStatus::Empty | PaneStatus::Ready { .. } => None,
                PaneStatus::Decoding { path, .. } => Some((
                    format!("Decoding {}…", file_display_name(path)),
                    normal_text,
                )),
                PaneStatus::Uploading { .. } => {
                    let progress = runtime
                        .image_id
                        .and_then(|image_id| self.renderer.upload_progress(image_id))
                        .map_or(0.0, |progress| progress.fraction());
                    Some((
                        format!("Uploading to GPU · {:.0}%", progress * 100.0),
                        normal_text,
                    ))
                }
                PaneStatus::Error { message } => Some((
                    format!("Could not open image\n{message}"),
                    Color32::LIGHT_RED,
                )),
            }
        };
        if let Some((text, color)) = status {
            ui.painter().with_clip_rect(area.rect.shrink(4.0)).text(
                area.rect.center(),
                Align2::CENTER_CENTER,
                text,
                FontId::proportional(14.0),
                color,
            );
        }

        if let PaneStatus::Ready {
            decode_time,
            total_bytes,
            source_size,
            quality,
            bit_depth,
        } = &runtime.status
        {
            let source_megapixels =
                f64::from(source_size[0]) * f64::from(source_size[1]) / 1_000_000.0;
            let quality = match quality {
                DecodeQuality::Full => "FULL",
                DecodeQuality::FullRaw => "FULL RAW",
                DecodeQuality::EmbeddedPreview => "EMBEDDED PREVIEW",
            };
            let detail = format!(
                "{} · {:.1} MP · {} bit · {:.0} ms · {:.1} MiB",
                quality,
                source_megapixels,
                bit_depth.unwrap_or_default(),
                decode_time.as_secs_f64() * 1000.0,
                *total_bytes as f64 / (1024.0 * 1024.0)
            );
            ui.painter().with_clip_rect(area.rect.shrink(4.0)).text(
                pos2(area.rect.left() + 8.0, area.rect.bottom() - 8.0),
                Align2::LEFT_BOTTOM,
                detail,
                FontId::monospace(10.0),
                normal_text.gamma_multiply(0.55),
            );
        }
    }

    fn is_busy(&self) -> bool {
        self.pane_runtime.values().any(|runtime| {
            matches!(
                runtime.status,
                PaneStatus::Decoding { .. } | PaneStatus::Uploading { .. }
            ) || runtime.full_raw_pending
        })
    }
}

impl Drop for DesktopApp {
    fn drop(&mut self) {
        for runtime in self.pane_runtime.values_mut() {
            runtime.handle.take();
        }
    }
}

impl eframe::App for DesktopApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(
            storage,
            PREFERENCES_KEY,
            &PersistedPreferences::capture(&self.workspace, &self.ui_state),
        );
    }

    fn auto_save_interval(&self) -> Duration {
        Duration::from_secs(5)
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.process_loader_results();
        self.update_uploads();
        self.continue_exposure_match();

        let dropped_paths: Vec<_> = ui.ctx().input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .filter_map(|file| file.path.clone())
                .collect()
        });
        self.open_paths(dropped_paths);

        self.ui_state.active_is_raw = self.workspace.active_pane.is_some_and(|pane_id| {
            self.pane_runtime
                .get(&pane_id)
                .is_some_and(|runtime| runtime.is_raw_source)
        });

        let output = ui_egui::draw_workspace(ui, &mut self.workspace, &mut self.ui_state);
        self.apply_pane_changes(&output);
        for &pane_id in &output.closed_images {
            self.close_image(pane_id);
        }
        if output.view_one_to_one_requested {
            self.request_full_raw_for_active_pane();
        }
        if let Some(mode) = output.raw_develop_requested {
            self.request_selected_raw_mode(mode);
        }
        if output.exposure_match_requested {
            self.begin_exposure_match(&output.paint_areas);
        }
        if output.preview_match_requested {
            self.match_active_raw_to_preview();
        }
        if output.exposure_match_reset_requested {
            self.reset_exposure_match();
        }
        self.request_raws_past_preview_resolution();
        self.add_image_callbacks(ui, &output);
        if output.open_requested {
            self.open_dialog();
        }
        if let Some(pane_id) = output.replace_image_requested {
            self.open_dialog_for(pane_id);
        }

        if self.is_busy() {
            ui.ctx().request_repaint_after(Duration::from_millis(16));
        }
    }

    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        if visuals.dark_mode {
            Color32::from_rgb(16, 18, 22).to_normalized_gamma_f32()
        } else {
            Color32::from_rgb(229, 233, 237).to_normalized_gamma_f32()
        }
    }
}
