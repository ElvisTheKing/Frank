use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
    time::Duration,
};

use eframe::egui::{self, Align2, Color32, FontId, pos2};
use image_loader::{DecodeQuality, DecodeReservation, ImageLoader, RawDevelopOptions};
use renderer_wgpu::{PaneRenderState, TileRenderer, UploadImage, UploadTile};
use rfd::FileDialog;
use ui_egui::{
    AlignmentDiagnosticMatch, AlignmentDiagnosticOverlay, AlignmentQuality, ComparisonMode,
    ManualRegistrationPoints, RegistrationRequest, UiState,
};
use viewer_model::{ImageId, ImageMetadata, MAX_PANES, Pane, PaneId, Workspace};

use crate::{
    comparison::{exposure_match_ev, fit_preview_curve, visible_region_luminance},
    pane_runtime::{PaneRuntime, PaneStatus, file_display_name},
    preferences::{PREFERENCES_KEY, PersistedPreferences},
    raw_pipeline::{
        full_raw_satisfies_resolution_request, needs_full_raw_development,
        preview_detail_exhausted, raw_options_match, raw_recipe_matches,
    },
    registration::{
        AutoRegistrationDiagnostics, AutoRegistrationEstimate, AutoRegistrationFailure,
        estimate_registration,
    },
    workspace_batch::resize_workspace_for_batch,
};

const APP_NAME: &str = "Frank";
const APP_ID: &str = "org.imagecomparetool.desktop";
const GPU_UPLOAD_BUDGET_PER_FRAME: usize = 32 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PendingRegistration {
    reference_pane: PaneId,
    reference_image: ImageId,
    target_image: ImageId,
}

#[derive(Clone, Debug)]
struct AutomaticRegistrationResult {
    target_pane: PaneId,
    pending: PendingRegistration,
    estimate: Result<AutoRegistrationEstimate, AutoRegistrationFailure>,
}

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
    pending_preview_matches: HashSet<PaneId>,
    registration_sender: Sender<AutomaticRegistrationResult>,
    registration_receiver: Receiver<AutomaticRegistrationResult>,
    pending_registrations: HashMap<PaneId, PendingRegistration>,
    alignment_diagnostics: Vec<AlignmentDiagnosticOverlay>,
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
        let (registration_sender, registration_receiver) = mpsc::channel();
        let mut app = Self {
            workspace,
            ui_state,
            pane_runtime,
            loader: ImageLoader::new(worker_count),
            renderer: TileRenderer::new(render_state),
            next_image_id: 1,
            pending_preview_matches: HashSet::new(),
            registration_sender,
            registration_receiver,
            pending_registrations: HashMap::new(),
            alignment_diagnostics: Vec::new(),
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
        self.ui_state.show_all_panes();
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
        self.cancel_automatic_registration_for(pane_id);
        self.pending_preview_matches.remove(&pane_id);
        if let Some(mut runtime) = self.pane_runtime.remove(&pane_id) {
            runtime.handle.take();
            if let Some(image_id) = runtime.image_id.take() {
                self.renderer.remove_image(image_id);
            }
        }
    }

    fn close_image(&mut self, pane_id: PaneId) {
        self.ui_state.cancel_note_edit_for(pane_id);
        self.ui_state.cancel_registration_for(pane_id);
        self.ui_state.cancel_focus_for(pane_id);
        self.remove_pane_runtime(pane_id);
        self.pane_runtime.insert(pane_id, PaneRuntime::default());
        self.workspace
            .clear_image(pane_id, format!("Pane {}", pane_id.0));
    }

    fn open_path(&mut self, pane_id: PaneId, path: PathBuf) {
        self.ui_state.cancel_note_edit_for(pane_id);
        self.ui_state.cancel_registration_for(pane_id);
        self.cancel_automatic_registration_for(pane_id);
        self.pending_preview_matches.remove(&pane_id);
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
        runtime.registration_image = None;
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
            pane.preview_match_gamma = 1.0;
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
            self.request_raw_development(pane_id);
        }
    }

    fn request_raw_development(&mut self, pane_id: PaneId) {
        let Some(runtime) = self.pane_runtime.get_mut(&pane_id) else {
            return;
        };
        if !runtime.is_raw_source {
            return;
        }
        let options = RawDevelopOptions::default();
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
                    runtime.raw_recipe = decoded.raw_recipe.clone();
                    runtime.raw_diagnostics = decoded.raw_diagnostics.clone();
                    runtime.display_linear_stats =
                        Some(decoded.display_linear_luminance_percentiles);
                    runtime.luminance_grid = Some(decoded.luminance_grid.clone());
                    let should_replace_registration_image =
                        runtime.registration_image.as_ref().is_none_or(|current| {
                            decoded.registration_image.width * decoded.registration_image.height
                                > current.width * current.height
                        });
                    if should_replace_registration_image {
                        runtime.registration_image = Some(decoded.registration_image.clone());
                    }
                    if quality == DecodeQuality::EmbeddedPreview {
                        runtime.preview_linear_stats = runtime.display_linear_stats;
                    } else if quality == DecodeQuality::FullRaw
                        && self.ui_state.match_raw_to_preview
                        && runtime.preview_linear_stats.is_some()
                    {
                        self.pending_preview_matches.insert(pane_id);
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
                            DecodeQuality::FullRaw => Some("full RAW".to_owned()),
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
            self.request_raw_development(pane_id);
        }
    }

    fn request_active_raw_development(&mut self) {
        let Some(pane_id) = self.workspace.active_pane else {
            return;
        };
        self.request_raw_development(pane_id);
    }

    fn request_all_raw_development(&mut self) {
        let targets = self
            .pane_runtime
            .iter()
            .filter(|(_, runtime)| {
                needs_full_raw_development(
                    runtime.is_raw_source,
                    runtime.full_raw_pending,
                    runtime.quality,
                )
            })
            .map(|(&pane_id, _)| pane_id)
            .collect::<Vec<_>>();
        for pane_id in targets {
            self.request_raw_development(pane_id);
        }
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
        for pane_id in requests {
            self.request_raw_development(pane_id);
        }
    }

    fn begin_exposure_match(&mut self, paint_areas: &[ui_egui::PanePaintArea]) {
        let Some(reference) = self.workspace.reference_pane_id() else {
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
        let reference_area = paint_areas
            .iter()
            .find(|area| area.pane_id == reference)
            .or_else(|| {
                ui_egui::focused_comparison_panes(&self.workspace, &self.ui_state)
                    .and_then(|(_, target)| paint_areas.iter().find(|area| area.pane_id == target))
            });
        let Some(reference_area) = reference_area else {
            return;
        };
        let Some(reference_sample) = self.pane_runtime.get(&reference).and_then(|runtime| {
            runtime.luminance_grid.as_ref().and_then(|grid| {
                visible_region_luminance(
                    grid,
                    reference_pane,
                    reference_area,
                    reference_pane.display_gamma(),
                    reference_pane.preview_match_ev + reference_pane.manual_exposure_ev,
                )
            })
        }) else {
            self.reset_normalization();
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
            let target_sample = self.pane_runtime.get(&target_pane.id).and_then(|runtime| {
                runtime.luminance_grid.as_ref().and_then(|grid| {
                    visible_region_luminance(
                        grid,
                        target_pane,
                        target_area,
                        target_pane.display_gamma(),
                        target_pane.preview_match_ev + target_pane.manual_exposure_ev,
                    )
                })
            });
            if let Some((target_median, target_confidence)) = target_sample {
                results.push((
                    target_pane.id,
                    exposure_match_ev(reference_sample.0, target_median),
                    reference_sample.1.min(target_confidence),
                ));
            }
        }
        self.reset_normalization();
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

    fn continue_preview_matches(&mut self) {
        let pending = self
            .pending_preview_matches
            .iter()
            .copied()
            .collect::<Vec<_>>();
        for pane_id in pending {
            let Some(runtime) = self.pane_runtime.get(&pane_id) else {
                self.pending_preview_matches.remove(&pane_id);
                continue;
            };
            if runtime.full_raw_pending {
                continue;
            }
            if runtime.quality == Some(DecodeQuality::FullRaw)
                && let (Some(preview), Some(current)) =
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
            self.pending_preview_matches.remove(&pane_id);
        }
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
        self.pending_preview_matches.insert(pane_id);
        if !matches!(
            runtime.status,
            PaneStatus::Ready {
                quality: DecodeQuality::FullRaw,
                ..
            }
        ) {
            self.request_raw_development(pane_id);
        }
    }

    fn set_preview_matching_enabled(&mut self, enabled: bool) {
        if enabled {
            self.pending_preview_matches.extend(
                self.pane_runtime
                    .iter()
                    .filter(|(_, runtime)| {
                        runtime.is_raw_source
                            && runtime.quality == Some(DecodeQuality::FullRaw)
                            && runtime.preview_linear_stats.is_some()
                    })
                    .map(|(&pane_id, _)| pane_id),
            );
        } else {
            self.pending_preview_matches.clear();
            for pane in &mut self.workspace.panes {
                pane.preview_match_ev = 0.0;
                pane.preview_match_gamma = 1.0;
            }
        }
    }

    fn reset_normalization(&mut self) {
        for pane in &mut self.workspace.panes {
            pane.exposure_match_ev = 0.0;
            pane.normalization_confidence = None;
        }
    }

    fn cancel_automatic_registration_for(&mut self, pane_id: PaneId) {
        self.pending_registrations
            .retain(|target, pending| *target != pane_id && pending.reference_pane != pane_id);
        self.alignment_diagnostics.retain(|diagnostics| {
            diagnostics.target_pane != pane_id && diagnostics.reference_pane != pane_id
        });
        self.ui_state.registration_busy = !self.pending_registrations.is_empty();
    }

    fn start_automatic_registration(&mut self, target_pane: PaneId) -> bool {
        let Some(reference_pane) = self.workspace.reference_pane_id() else {
            self.ui_state.registration_status = Some("Choose a reference pane first".to_owned());
            return false;
        };
        if reference_pane == target_pane {
            self.ui_state.registration_status =
                Some("The reference pane is already the alignment anchor".to_owned());
            return false;
        }
        let Some(reference_runtime) = self.pane_runtime.get(&reference_pane) else {
            self.ui_state.registration_status =
                Some("The reference pane has no decoded image".to_owned());
            return false;
        };
        let Some(target_runtime) = self.pane_runtime.get(&target_pane) else {
            self.ui_state.registration_status =
                Some("The target pane has no decoded image".to_owned());
            return false;
        };
        let (Some(reference_image), Some(reference_registration_image)) = (
            reference_runtime.image_id,
            reference_runtime.registration_image.clone(),
        ) else {
            self.ui_state.registration_status =
                Some("Wait for the reference image to finish loading".to_owned());
            return false;
        };
        let (Some(target_image), Some(target_registration_image)) = (
            target_runtime.image_id,
            target_runtime.registration_image.clone(),
        ) else {
            self.ui_state.registration_status =
                Some("Wait for the target image to finish loading".to_owned());
            return false;
        };
        let pending = PendingRegistration {
            reference_pane,
            reference_image,
            target_image,
        };
        if self.pending_registrations.get(&target_pane) == Some(&pending) {
            self.ui_state.registration_status =
                Some(format!("Pane {} is already being aligned…", target_pane.0));
            return false;
        }

        self.alignment_diagnostics
            .retain(|diagnostics| diagnostics.target_pane != target_pane);
        self.ui_state.alignment_quality = None;
        self.pending_registrations.insert(target_pane, pending);
        self.ui_state.registration_busy = true;
        self.ui_state.registration_status = Some(format!("Aligning pane {}…", target_pane.0));
        let sender = self.registration_sender.clone();
        let spawn_result = std::thread::Builder::new()
            .name(format!("registration-{}", target_pane.0))
            .spawn(move || {
                let estimate = estimate_registration(
                    &reference_registration_image,
                    &target_registration_image,
                );
                let _ = sender.send(AutomaticRegistrationResult {
                    target_pane,
                    pending,
                    estimate,
                });
            });
        if let Err(error) = spawn_result {
            self.pending_registrations.remove(&target_pane);
            self.ui_state.registration_busy = !self.pending_registrations.is_empty();
            self.ui_state.registration_status = Some(format!("Could not start alignment: {error}"));
            return false;
        }
        true
    }

    fn start_automatic_registration_for_all(&mut self) {
        let reference = self.workspace.reference_pane_id();
        let targets = self
            .workspace
            .panes
            .iter()
            .filter(|pane| Some(pane.id) != reference && pane.image_id.is_some())
            .map(|pane| pane.id)
            .collect::<Vec<_>>();
        let started = targets
            .into_iter()
            .filter(|target| self.start_automatic_registration(*target))
            .count();
        if started > 0 {
            self.ui_state.registration_status = Some(format!("Aligning {started} target pane(s)…"));
        } else if self.ui_state.registration_status.is_none() {
            self.ui_state.registration_status = Some("No loaded target panes to align".to_owned());
        }
    }

    fn apply_manual_registration(&mut self, points: ManualRegistrationPoints) {
        self.alignment_diagnostics
            .retain(|diagnostics| diagnostics.target_pane != points.target_pane);
        self.ui_state.alignment_quality = None;
        if self.workspace.reference_pane_id() != Some(points.reference_pane) {
            self.ui_state.registration_status =
                Some("Reference pane changed; manual alignment was discarded".to_owned());
            return;
        }
        match self.workspace.align_pane_from_points(
            points.reference_pane,
            points.target_pane,
            points.reference_points,
            points.target_points,
        ) {
            Ok(outcome) => {
                let rotation_note = if outcome.rotation_degrees.abs() >= 1.0 {
                    format!(
                        " · measured rotation {:+.1}° (not applied)",
                        outcome.rotation_degrees
                    )
                } else {
                    String::new()
                };
                self.ui_state.registration_status = Some(format!(
                    "Pane {} aligned · scale {:.3}×{}",
                    points.target_pane.0, outcome.scale_ratio, rotation_note
                ));
            }
            Err(error) => {
                self.ui_state.registration_status =
                    Some(format!("Manual alignment failed: {error}"));
            }
        }
    }

    fn handle_registration_request(&mut self, request: RegistrationRequest) {
        match request {
            RegistrationRequest::SetReference(pane_id) => {
                if let Some(previous) = self.workspace.reference_pane_id() {
                    self.ui_state.cancel_registration_for(previous);
                }
                self.pending_registrations.clear();
                self.alignment_diagnostics.clear();
                self.ui_state.alignment_quality = None;
                self.ui_state.registration_busy = false;
                match self.workspace.set_reference_pane(pane_id) {
                    Ok(()) => {
                        self.ui_state.registration_status =
                            Some(format!("Pane {} is now the reference", pane_id.0));
                    }
                    Err(error) => {
                        self.ui_state.registration_status =
                            Some(format!("Could not set reference pane: {error}"));
                    }
                }
            }
            RegistrationRequest::Automatic(pane_id) => {
                self.start_automatic_registration(pane_id);
            }
            RegistrationRequest::AutomaticAll => self.start_automatic_registration_for_all(),
            RegistrationRequest::Reset(pane_id) => {
                self.cancel_automatic_registration_for(pane_id);
                match self.workspace.reset_pane_registration(pane_id) {
                    Ok(()) => {
                        self.ui_state.registration_status =
                            Some(format!("Pane {} alignment reset", pane_id.0));
                    }
                    Err(error) => {
                        self.ui_state.registration_status =
                            Some(format!("Could not reset alignment: {error}"));
                    }
                }
            }
            RegistrationRequest::ResetAll => {
                self.pending_registrations.clear();
                self.alignment_diagnostics.clear();
                self.ui_state.alignment_quality = None;
                self.ui_state.registration_busy = false;
                self.workspace.reset_sync_adjustments();
                self.ui_state.registration_status =
                    Some("All alignment adjustments reset".to_owned());
            }
        }
    }

    fn record_alignment_diagnostics(
        &mut self,
        reference_pane: PaneId,
        target_pane: PaneId,
        succeeded: bool,
        diagnostics: &AutoRegistrationDiagnostics,
    ) {
        self.alignment_diagnostics
            .retain(|overlay| overlay.target_pane != target_pane);
        if !diagnostics.matches.is_empty() {
            self.alignment_diagnostics.push(AlignmentDiagnosticOverlay {
                reference_pane,
                target_pane,
                matches: diagnostics
                    .matches
                    .iter()
                    .map(|feature_match| AlignmentDiagnosticMatch {
                        reference: feature_match.reference,
                        target: feature_match.target,
                        inlier: feature_match.inlier,
                    })
                    .collect(),
            });
        }
        self.ui_state.alignment_quality = Some(AlignmentQuality {
            target_pane,
            succeeded,
            confidence: diagnostics.confidence,
            reference_features: diagnostics.reference_features,
            target_features: diagnostics.target_features,
            candidate_matches: diagnostics.candidate_matches,
            inliers: diagnostics.inliers,
            median_error_pixels: diagnostics.median_error_pixels,
        });
    }

    fn process_automatic_registration_results(&mut self) {
        while let Ok(result) = self.registration_receiver.try_recv() {
            let Some(pending) = self.pending_registrations.get(&result.target_pane).copied() else {
                continue;
            };
            if pending != result.pending {
                continue;
            }
            self.pending_registrations.remove(&result.target_pane);
            let reference_is_current = self.workspace.reference_pane_id()
                == Some(result.pending.reference_pane)
                && self
                    .pane_runtime
                    .get(&result.pending.reference_pane)
                    .and_then(|runtime| runtime.image_id)
                    == Some(result.pending.reference_image);
            let target_is_current = self
                .pane_runtime
                .get(&result.target_pane)
                .and_then(|runtime| runtime.image_id)
                == Some(result.pending.target_image);
            if !reference_is_current || !target_is_current {
                self.ui_state.registration_status =
                    Some("An image changed; stale alignment was discarded".to_owned());
                continue;
            }
            match result.estimate {
                Ok(estimate) => {
                    match self.workspace.align_pane_from_points(
                        result.pending.reference_pane,
                        result.target_pane,
                        estimate.reference_points,
                        estimate.target_points,
                    ) {
                        Ok(_) => {
                            self.record_alignment_diagnostics(
                                result.pending.reference_pane,
                                result.target_pane,
                                true,
                                &estimate.diagnostics,
                            );
                            self.ui_state.registration_status = Some(format!(
                                "Pane {} aligned · {:.0}% confidence · {} features · {}/{} inliers · {:.1}px error · scale {:.3}× · offset {:+.3}, {:+.3}",
                                result.target_pane.0,
                                estimate.confidence * 100.0,
                                estimate.diagnostics.reference_features
                                    + estimate.diagnostics.target_features,
                                estimate.diagnostics.inliers,
                                estimate.diagnostics.candidate_matches,
                                estimate.median_error_pixels,
                                estimate.mapping_scale,
                                estimate.translation.x,
                                estimate.translation.y
                            ));
                        }
                        Err(error) => {
                            self.record_alignment_diagnostics(
                                result.pending.reference_pane,
                                result.target_pane,
                                false,
                                &estimate.diagnostics,
                            );
                            self.ui_state.registration_status =
                                Some(format!("Auto alignment failed: {error}"));
                        }
                    }
                }
                Err(failure) => {
                    self.record_alignment_diagnostics(
                        result.pending.reference_pane,
                        result.target_pane,
                        false,
                        &failure.diagnostics,
                    );
                    self.ui_state.registration_status = Some(format!(
                        "Pane {} not aligned [{}]: {} · features {}/{} · {} matches · {} inliers; try manual alignment",
                        result.target_pane.0,
                        failure.reason.code(),
                        failure.reason,
                        failure.diagnostics.reference_features,
                        failure.diagnostics.target_features,
                        failure.diagnostics.candidate_matches,
                        failure.diagnostics.inliers
                    ));
                }
            }
        }
        self.ui_state.registration_busy = !self.pending_registrations.is_empty();
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
        let reference_pane = self.workspace.reference_pane_id();
        let comparison_panes = ui_egui::focused_comparison_panes(&self.workspace, &self.ui_state);
        let reference_metadata = reference_pane.and_then(|reference| {
            self.workspace
                .panes
                .iter()
                .find(|pane| pane.id == reference)
                .map(|pane| &pane.metadata)
        });
        for area in &output.paint_areas {
            let Some(pane) = self
                .workspace
                .panes
                .iter()
                .find(|pane| pane.id == area.pane_id)
            else {
                continue;
            };
            let full_clip = [0.0, 0.0, area.physical_size[0], area.physical_size[1]];
            if comparison_panes.is_some_and(|(_, target)| target == pane.id) {
                let (reference, _) = comparison_panes.expect("comparison pair exists");
                let reference = self
                    .workspace
                    .panes
                    .iter()
                    .find(|candidate| candidate.id == reference)
                    .expect("comparison reference exists");
                match self.ui_state.comparison_mode() {
                    ComparisonMode::Normal if self.ui_state.blink_reference_visible() => {
                        self.add_pane_image_callback(ui, area, reference, pane.id, 1, full_clip);
                    }
                    ComparisonMode::VerticalSplit => {
                        let divider = area.physical_size[0]
                            * self.ui_state.split_position().clamp(0.02, 0.98);
                        self.add_pane_image_callback(
                            ui,
                            area,
                            reference,
                            pane.id,
                            1,
                            [0.0, 0.0, divider, area.physical_size[1]],
                        );
                        self.add_pane_image_callback(
                            ui,
                            area,
                            pane,
                            pane.id,
                            2,
                            [divider, 0.0, area.physical_size[0], area.physical_size[1]],
                        );
                    }
                    ComparisonMode::HorizontalSplit => {
                        let divider = area.physical_size[1]
                            * self.ui_state.split_position().clamp(0.02, 0.98);
                        self.add_pane_image_callback(
                            ui,
                            area,
                            reference,
                            pane.id,
                            1,
                            [0.0, 0.0, area.physical_size[0], divider],
                        );
                        self.add_pane_image_callback(
                            ui,
                            area,
                            pane,
                            pane.id,
                            2,
                            [0.0, divider, area.physical_size[0], area.physical_size[1]],
                        );
                    }
                    ComparisonMode::Normal => {
                        self.add_pane_image_callback(ui, area, pane, pane.id, 0, full_clip);
                    }
                }
            } else {
                self.add_pane_image_callback(ui, area, pane, pane.id, 0, full_clip);
            }
            if let Some(runtime) = self.pane_runtime.get(&pane.id) {
                self.paint_status(ui, area, runtime);
            }
            if !self.ui_state.show_pane_controls && pane.image_id.is_some() {
                let mut label =
                    pane.formatted_title_relative(self.workspace.title_fields, reference_metadata);
                if Some(pane.id) == reference_pane {
                    label.insert_str(0, "REF · ");
                }
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

    fn add_pane_image_callback(
        &self,
        ui: &mut egui::Ui,
        area: &ui_egui::PanePaintArea,
        source: &Pane,
        render_pane_id: PaneId,
        render_slot: u8,
        clip_rect: [f32; 4],
    ) {
        let Some(image_id) = source.image_id else {
            return;
        };
        ui.painter().add(self.renderer.paint_callback(
            area.rect,
            PaneRenderState {
                pane_id: render_pane_id,
                render_slot,
                image_id,
                center: [
                    source.viewport.center.x as f32,
                    source.viewport.center.y as f32,
                ],
                source_size: source.image_size.unwrap_or([1, 1]),
                source_pixels_per_physical_pixel: source.viewport.source_pixels_per_physical_pixel
                    as f32,
                physical_size: area.physical_size,
                clip_rect,
                exposure_ev: source.display_exposure_ev(),
                gamma: source.display_gamma(),
            },
        ));
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
        !self.pending_registrations.is_empty()
            || self.pane_runtime.values().any(|runtime| {
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
        self.continue_preview_matches();
        self.process_automatic_registration_results();

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
        self.ui_state.has_raw_images = self
            .pane_runtime
            .values()
            .any(|runtime| runtime.is_raw_source);

        let output = ui_egui::draw_workspace(ui, &mut self.workspace, &mut self.ui_state);
        self.apply_pane_changes(&output);
        for &pane_id in &output.closed_images {
            self.close_image(pane_id);
        }
        if output.view_one_to_one_requested {
            self.request_full_raw_for_active_pane();
        }
        if output.raw_develop_requested {
            self.request_active_raw_development();
        }
        if output.raw_develop_all_requested {
            self.request_all_raw_development();
        }
        if output.exposure_match_requested {
            self.begin_exposure_match(&output.paint_areas);
        }
        if output.preview_match_requested {
            self.match_active_raw_to_preview();
        }
        if let Some(enabled) = output.preview_match_enabled_changed {
            self.set_preview_matching_enabled(enabled);
        }
        if output.exposure_match_reset_requested {
            self.reset_normalization();
        }
        if let Some(request) = output.registration_request {
            self.handle_registration_request(request);
        }
        if let Some(points) = output.manual_registration_completed {
            self.apply_manual_registration(points);
        }
        self.request_raws_past_preview_resolution();
        self.add_image_callbacks(ui, &output);
        ui_egui::paint_registration_overlays(
            ui,
            &self.workspace,
            &self.ui_state,
            &output.paint_areas,
            &self.alignment_diagnostics,
        );
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
