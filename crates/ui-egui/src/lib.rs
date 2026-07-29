#![forbid(unsafe_code)]

use egui::{Align2, Color32, CursorIcon, FontId, Rect, Sense, Stroke, StrokeKind, Ui, Vec2, pos2};
use renderer_wgpu::{PaneLayout, WorkspaceLayout};
use serde::{Deserialize, Serialize};
use viewer_model::{
    LayoutMode, MAX_NOTE_CHARS, MAX_PANES, MIN_PANES, NormalizedPoint, PaneId, SyncMode,
    TitleFields, Workspace,
};

const TOOLBAR_HEIGHT: f32 = 44.0;
const TOOLBAR_MARGIN: f32 = 8.0;
const TITLE_HEIGHT: f32 = 46.0;
const REFERENCE_CONTROL_WIDTH: f32 = 40.0;
const LINK_CONTROL_WIDTH: f32 = 46.0;
const NOTE_CONTROL_WIDTH: f32 = 28.0;
const FOCUS_CONTROL_WIDTH: f32 = 28.0;
const CLOSE_CONTROL_WIDTH: f32 = 28.0;
const PANE_GAP: f32 = 1.0;
const MIN_PIXEL_GRID_SIZE_PHYSICAL: f64 = 6.0;
const MAX_PIXEL_GRID_LINES_PER_AXIS: usize = 2_048;

#[derive(Debug)]
pub struct UiState {
    pub show_pixel_grid: bool,
    pub show_pane_controls: bool,
    pub develop_raws_on_load: bool,
    pub match_raw_to_preview: bool,
    pub active_is_raw: bool,
    pub has_raw_images: bool,
    pub sync_adjustments: bool,
    pub theme: AppTheme,
    pub registration_busy: bool,
    pub registration_status: Option<String>,
    pub alignment_quality: Option<AlignmentQuality>,
    pub show_alignment_diagnostics: bool,
    comparison_mode: ComparisonMode,
    split_position: f32,
    blink_latched: bool,
    blink_held: bool,
    split_dragging: Option<PaneId>,
    focused_pane: Option<PaneId>,
    dragged_pane: Option<PaneId>,
    drop_target: Option<PaneId>,
    note_editor: Option<NoteEditor>,
    manual_registration: Option<ManualRegistrationSession>,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            show_pixel_grid: false,
            show_pane_controls: true,
            develop_raws_on_load: false,
            match_raw_to_preview: true,
            active_is_raw: false,
            has_raw_images: false,
            sync_adjustments: false,
            theme: AppTheme::default(),
            registration_busy: false,
            registration_status: None,
            alignment_quality: None,
            show_alignment_diagnostics: false,
            comparison_mode: ComparisonMode::Normal,
            split_position: 0.5,
            blink_latched: false,
            blink_held: false,
            split_dragging: None,
            focused_pane: None,
            dragged_pane: None,
            drop_target: None,
            note_editor: None,
            manual_registration: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ComparisonMode {
    #[default]
    Normal,
    VerticalSplit,
    HorizontalSplit,
}

impl ComparisonMode {
    #[must_use]
    pub const fn is_split(self) -> bool {
        matches!(self, Self::VerticalSplit | Self::HorizontalSplit)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum AppTheme {
    #[default]
    System,
    Dark,
    Light,
}

impl AppTheme {
    const fn egui_theme_preference(self) -> egui::ThemePreference {
        match self {
            Self::System => egui::ThemePreference::System,
            Self::Dark => egui::ThemePreference::Dark,
            Self::Light => egui::ThemePreference::Light,
        }
    }
}

#[derive(Clone, Copy)]
struct UiPalette {
    toolbar: Color32,
    toolbar_border: Color32,
    separator: Color32,
    pane: Color32,
    header: Color32,
    image: Color32,
    header_border: Color32,
    active: Color32,
    primary_text: Color32,
    secondary_text: Color32,
    zoom_text: Color32,
    placeholder_text: Color32,
    link_on: Color32,
    link_off: Color32,
    note_empty: Color32,
    note_filled: Color32,
    note_hover: Color32,
}

fn palette(theme: egui::Theme) -> UiPalette {
    match theme {
        egui::Theme::Dark => UiPalette {
            toolbar: Color32::from_rgb(24, 29, 34),
            toolbar_border: Color32::from_rgb(48, 56, 64),
            separator: Color32::from_rgb(61, 67, 74),
            pane: Color32::from_rgb(16, 19, 23),
            header: Color32::from_rgb(29, 34, 40),
            image: Color32::from_rgb(12, 15, 18),
            header_border: Color32::from_rgb(48, 55, 62),
            active: Color32::from_rgb(76, 145, 196),
            primary_text: Color32::from_gray(232),
            secondary_text: Color32::from_gray(156),
            zoom_text: Color32::from_rgb(152, 190, 222),
            placeholder_text: Color32::from_gray(92),
            link_on: Color32::from_rgb(45, 78, 101),
            link_off: Color32::from_rgb(55, 58, 64),
            note_empty: Color32::from_rgb(48, 52, 59),
            note_filled: Color32::from_rgb(48, 90, 125),
            note_hover: Color32::from_rgb(75, 82, 92),
        },
        egui::Theme::Light => UiPalette {
            toolbar: Color32::from_rgb(239, 242, 245),
            toolbar_border: Color32::from_rgb(191, 199, 207),
            separator: Color32::from_rgb(164, 172, 180),
            pane: Color32::from_rgb(229, 233, 237),
            header: Color32::from_rgb(244, 246, 248),
            image: Color32::from_rgb(217, 222, 227),
            header_border: Color32::from_rgb(196, 203, 210),
            active: Color32::from_rgb(36, 108, 164),
            primary_text: Color32::from_rgb(31, 37, 43),
            secondary_text: Color32::from_rgb(89, 98, 107),
            zoom_text: Color32::from_rgb(41, 102, 150),
            placeholder_text: Color32::from_rgb(119, 127, 135),
            link_on: Color32::from_rgb(62, 111, 145),
            link_off: Color32::from_rgb(205, 211, 217),
            note_empty: Color32::from_rgb(213, 218, 223),
            note_filled: Color32::from_rgb(104, 153, 188),
            note_hover: Color32::from_rgb(190, 199, 207),
        },
    }
}

impl UiState {
    #[must_use]
    pub const fn comparison_mode(&self) -> ComparisonMode {
        self.comparison_mode
    }

    #[must_use]
    pub const fn split_position(&self) -> f32 {
        self.split_position
    }

    #[must_use]
    pub const fn blink_reference_visible(&self) -> bool {
        matches!(self.comparison_mode, ComparisonMode::Normal)
            && (self.blink_latched || self.blink_held)
    }

    #[must_use]
    pub const fn focused_pane(&self) -> Option<PaneId> {
        self.focused_pane
    }

    pub fn show_all_panes(&mut self) {
        self.focused_pane = None;
    }

    pub fn cancel_note_edit_for(&mut self, pane_id: PaneId) {
        if self
            .note_editor
            .as_ref()
            .is_some_and(|editor| editor.pane_id == pane_id)
        {
            self.note_editor = None;
        }
    }

    pub fn cancel_registration_for(&mut self, pane_id: PaneId) {
        if self
            .manual_registration
            .as_ref()
            .is_some_and(|registration| {
                registration.reference_pane == pane_id || registration.target_pane == pane_id
            })
        {
            self.manual_registration = None;
        }
        if self.split_dragging == Some(pane_id) {
            self.split_dragging = None;
        }
    }

    pub fn cancel_focus_for(&mut self, pane_id: PaneId) {
        if self.focused_pane == Some(pane_id) {
            self.focused_pane = None;
        }
    }

    fn start_manual_registration(&mut self, reference_pane: PaneId, target_pane: PaneId) {
        self.manual_registration = Some(ManualRegistrationSession {
            reference_pane,
            target_pane,
            samples: Vec::with_capacity(4),
        });
        self.registration_status = None;
    }

    fn expected_registration_pane(&self) -> Option<PaneId> {
        let registration = self.manual_registration.as_ref()?;
        Some(if registration.samples.len() % 2 == 0 {
            registration.reference_pane
        } else {
            registration.target_pane
        })
    }

    fn record_registration_point(
        &mut self,
        pane_id: PaneId,
        point: NormalizedPoint,
    ) -> Option<ManualRegistrationPoints> {
        let registration = self.manual_registration.as_mut()?;
        let expected_pane = if registration.samples.len() % 2 == 0 {
            registration.reference_pane
        } else {
            registration.target_pane
        };
        if expected_pane != pane_id {
            return None;
        }
        registration.samples.push((pane_id, point));
        if registration.samples.len() < 4 {
            return None;
        }
        let completed = ManualRegistrationPoints {
            reference_pane: registration.reference_pane,
            target_pane: registration.target_pane,
            reference_points: [registration.samples[0].1, registration.samples[2].1],
            target_points: [registration.samples[1].1, registration.samples[3].1],
        };
        self.manual_registration = None;
        Some(completed)
    }
}

#[derive(Debug)]
struct ManualRegistrationSession {
    reference_pane: PaneId,
    target_pane: PaneId,
    samples: Vec<(PaneId, NormalizedPoint)>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ManualRegistrationPoints {
    pub reference_pane: PaneId,
    pub target_pane: PaneId,
    pub reference_points: [NormalizedPoint; 2],
    pub target_points: [NormalizedPoint; 2],
}

#[derive(Clone, Debug, PartialEq)]
pub struct AlignmentQuality {
    pub target_pane: PaneId,
    pub succeeded: bool,
    pub confidence: Option<f32>,
    pub reference_features: usize,
    pub target_features: usize,
    pub candidate_matches: usize,
    pub inliers: usize,
    pub median_error_pixels: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AlignmentDiagnosticMatch {
    pub reference: NormalizedPoint,
    pub target: NormalizedPoint,
    pub inlier: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AlignmentDiagnosticOverlay {
    pub reference_pane: PaneId,
    pub target_pane: PaneId,
    pub matches: Vec<AlignmentDiagnosticMatch>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegistrationRequest {
    SetReference(PaneId),
    Automatic(PaneId),
    AutomaticAll,
    Reset(PaneId),
    ResetAll,
}

#[derive(Debug)]
struct NoteEditor {
    pane_id: PaneId,
    draft: String,
    request_focus: bool,
}

#[derive(Debug)]
pub struct UiOutput {
    pub layout: WorkspaceLayout,
    pub paint_areas: Vec<PanePaintArea>,
    pub open_requested: bool,
    pub replace_image_requested: Option<PaneId>,
    pub closed_images: Vec<PaneId>,
    pub added_panes: Vec<PaneId>,
    pub removed_panes: Vec<PaneId>,
    pub view_one_to_one_requested: bool,
    pub raw_develop_requested: bool,
    pub raw_develop_all_requested: bool,
    pub exposure_match_requested: bool,
    pub preview_match_requested: bool,
    pub preview_match_enabled_changed: Option<bool>,
    pub exposure_match_reset_requested: bool,
    pub registration_request: Option<RegistrationRequest>,
    pub manual_registration_completed: Option<ManualRegistrationPoints>,
}

#[derive(Default)]
struct ToolbarOutput {
    open_requested: bool,
    replace_image_requested: Option<PaneId>,
    closed_images: Vec<PaneId>,
    added_panes: Vec<PaneId>,
    removed_panes: Vec<PaneId>,
    view_one_to_one_requested: bool,
    raw_develop_requested: bool,
    raw_develop_all_requested: bool,
    exposure_match_requested: bool,
    preview_match_requested: bool,
    preview_match_enabled_changed: Option<bool>,
    exposure_match_reset_requested: bool,
    registration_request: Option<RegistrationRequest>,
    manual_registration_completed: Option<ManualRegistrationPoints>,
}

#[derive(Clone, Copy, Debug)]
pub struct PanePaintArea {
    pub pane_id: PaneId,
    pub rect: Rect,
    pub physical_size: [f32; 2],
}

#[derive(Clone, Copy)]
struct PaneDrawGeometry {
    rect: Rect,
    pixels_per_point: f32,
    colors: UiPalette,
}

pub fn draw_workspace(ui: &mut Ui, workspace: &mut Workspace, state: &mut UiState) -> UiOutput {
    ui.ctx().set_theme(state.theme.egui_theme_preference());
    if state
        .focused_pane
        .is_some_and(|focused| !workspace.panes.iter().any(|pane| pane.id == focused))
    {
        state.focused_pane = None;
    }
    if let Some(focused) = state.focused_pane {
        let _ = workspace.set_active(focused);
    }
    state.blink_held = focused_comparison_panes(workspace, state).is_some()
        && !ui.ctx().egui_wants_keyboard_input()
        && ui.input(|input| input.key_down(egui::Key::Space));
    let colors = palette(ui.ctx().theme());
    let full_rect = ui.max_rect();
    let pixels_per_point = ui.ctx().pixels_per_point();
    let toolbar_rect =
        Rect::from_min_size(full_rect.min, Vec2::new(full_rect.width(), TOOLBAR_HEIGHT));
    ui.painter().rect_filled(toolbar_rect, 0.0, colors.toolbar);
    ui.painter().line_segment(
        [toolbar_rect.left_bottom(), toolbar_rect.right_bottom()],
        Stroke::new(1.0, colors.toolbar_border),
    );
    let mut toolbar_output = ui
        .scope_builder(egui::UiBuilder::new().max_rect(toolbar_rect), |ui| {
            draw_toolbar(ui, workspace, state)
        })
        .inner;

    let workspace_rect = Rect::from_min_max(
        pos2(full_rect.left(), toolbar_rect.bottom()),
        full_rect.right_bottom(),
    );
    ui.painter()
        .rect_filled(workspace_rect, 0.0, colors.separator);

    let visible_pane_indices: Vec<_> = state.focused_pane.map_or_else(
        || (0..workspace.panes.len()).collect(),
        |focused| {
            workspace
                .panes
                .iter()
                .position(|pane| pane.id == focused)
                .into_iter()
                .collect()
        },
    );
    let pane_rects = grid_rects(
        workspace_rect,
        visible_pane_indices.len(),
        workspace.layout_mode,
    );
    let mut layout = WorkspaceLayout {
        physical_size: [
            (full_rect.width().max(0.0) * pixels_per_point).round() as u32,
            (full_rect.height().max(0.0) * pixels_per_point).round() as u32,
        ],
        panes: Vec::with_capacity(pane_rects.len()),
    };
    let mut paint_areas = Vec::with_capacity(pane_rects.len());
    if state.dragged_pane.is_some() {
        state.drop_target = None;
    }

    for (visible_index, pane_rect) in pane_rects.into_iter().enumerate() {
        let index = visible_pane_indices[visible_index];
        let pane_id = workspace.panes[index].id;
        let header_height = if state.show_pane_controls {
            TITLE_HEIGHT
        } else {
            0.0
        };
        let image_rect = Rect::from_min_max(
            pos2(pane_rect.left(), pane_rect.top() + header_height),
            pane_rect.right_bottom(),
        );
        let physical_size = [
            image_rect.width() * pixels_per_point,
            image_rect.height() * pixels_per_point,
        ];
        update_fit_scale_for_area(workspace, pane_id, physical_size);
        if state.focused_pane == Some(pane_id)
            && let Some(reference) = workspace.reference_pane_id()
            && reference != pane_id
        {
            update_fit_scale_for_area(workspace, reference, physical_size);
        }
        draw_pane(
            ui,
            workspace,
            state,
            pane_id,
            PaneDrawGeometry {
                rect: pane_rect,
                pixels_per_point,
                colors,
            },
            &mut toolbar_output,
        );
        layout.panes.push(PaneLayout {
            pane_id,
            physical_rect: rect_to_physical(image_rect, pixels_per_point),
        });
        paint_areas.push(PanePaintArea {
            pane_id,
            rect: image_rect,
            physical_size,
        });
    }

    let reference_shortcut_pressed = !ui.ctx().egui_wants_keyboard_input()
        && ui.input(|input| {
            input.key_pressed(egui::Key::R)
                && !input.modifiers.alt
                && !input.modifiers.ctrl
                && !input.modifiers.command
                && !input.modifiers.shift
        });
    if reference_shortcut_pressed
        && toolbar_output.registration_request.is_none()
        && let Some(pane_id) = reference_shortcut_target(workspace)
    {
        toolbar_output.registration_request = Some(RegistrationRequest::SetReference(pane_id));
    }

    let focus_shortcut_pressed = !ui.ctx().egui_wants_keyboard_input()
        && ui.input(|input| {
            input.key_pressed(egui::Key::F)
                && !input.modifiers.alt
                && !input.modifiers.ctrl
                && !input.modifiers.command
                && !input.modifiers.shift
        });
    if focus_shortcut_pressed {
        if state.focused_pane.is_some() {
            state.focused_pane = None;
        } else if let Some(active) = workspace.active_pane.filter(|active| {
            workspace
                .panes
                .iter()
                .any(|pane| pane.id == *active && pane.image_id.is_some())
        }) {
            maximize_pane(state, active);
        }
    }
    let leave_focus_pressed = state.focused_pane.is_some()
        && state.manual_registration.is_none()
        && !ui.ctx().egui_wants_keyboard_input()
        && ui.input(|input| input.key_pressed(egui::Key::Escape));
    if leave_focus_pressed {
        state.focused_pane = None;
    }

    if ui.input(|input| input.pointer.any_released()) {
        finish_pane_drag(workspace, state);
        state.split_dragging = None;
    }
    toolbar_output.removed_panes.dedup();
    let requested_removals = std::mem::take(&mut toolbar_output.removed_panes);
    for pane_id in requested_removals {
        if workspace.remove_pane(pane_id).is_ok() {
            state.cancel_note_edit_for(pane_id);
            state.cancel_registration_for(pane_id);
            state.cancel_focus_for(pane_id);
            if state.dragged_pane == Some(pane_id) {
                state.dragged_pane = None;
                state.drop_target = None;
            }
            toolbar_output.removed_panes.push(pane_id);
        }
    }
    draw_note_editor(ui.ctx(), workspace, state);

    UiOutput {
        layout,
        paint_areas,
        open_requested: toolbar_output.open_requested,
        replace_image_requested: toolbar_output.replace_image_requested,
        closed_images: toolbar_output.closed_images,
        added_panes: toolbar_output.added_panes,
        removed_panes: toolbar_output.removed_panes,
        view_one_to_one_requested: toolbar_output.view_one_to_one_requested,
        raw_develop_requested: toolbar_output.raw_develop_requested,
        raw_develop_all_requested: toolbar_output.raw_develop_all_requested,
        exposure_match_requested: toolbar_output.exposure_match_requested,
        preview_match_requested: toolbar_output.preview_match_requested,
        preview_match_enabled_changed: toolbar_output.preview_match_enabled_changed,
        exposure_match_reset_requested: toolbar_output.exposure_match_reset_requested,
        registration_request: toolbar_output.registration_request,
        manual_registration_completed: toolbar_output.manual_registration_completed,
    }
}

fn reference_shortcut_target(workspace: &Workspace) -> Option<PaneId> {
    let active = workspace.active_pane?;
    workspace
        .panes
        .iter()
        .any(|pane| pane.id == active && pane.image_id.is_some())
        .then_some(active)
}

#[must_use]
pub fn comparison_panes(workspace: &Workspace) -> Option<(PaneId, PaneId)> {
    let reference = workspace.reference_pane_id()?;
    let target = workspace.active_pane?;
    if reference == target {
        return None;
    }
    let reference_loaded = workspace
        .panes
        .iter()
        .any(|pane| pane.id == reference && pane.image_id.is_some());
    let target_loaded = workspace
        .panes
        .iter()
        .any(|pane| pane.id == target && pane.image_id.is_some());
    (reference_loaded && target_loaded).then_some((reference, target))
}

#[must_use]
pub fn focused_comparison_panes(
    workspace: &Workspace,
    state: &UiState,
) -> Option<(PaneId, PaneId)> {
    let comparison = comparison_panes(workspace)?;
    (state.focused_pane == Some(comparison.1)).then_some(comparison)
}

fn maximize_pane(state: &mut UiState, pane_id: PaneId) {
    state.focused_pane = Some(pane_id);
    state.comparison_mode = ComparisonMode::Normal;
    state.blink_latched = false;
}

pub fn paint_registration_overlays(
    ui: &Ui,
    workspace: &Workspace,
    state: &UiState,
    paint_areas: &[PanePaintArea],
    diagnostic_overlays: &[AlignmentDiagnosticOverlay],
) {
    if state.show_pixel_grid {
        paint_pixel_grids(ui, workspace, state, paint_areas);
    }

    if let Some(reference) = workspace.reference_pane_id()
        && let Some(area) = paint_areas.iter().find(|area| area.pane_id == reference)
    {
        ui.painter().rect_stroke(
            area.rect.shrink(0.75),
            0.0,
            Stroke::new(1.5, palette(ui.ctx().theme()).active),
            StrokeKind::Inside,
        );
    }

    if state.show_alignment_diagnostics {
        paint_automatic_alignment_diagnostics(ui, workspace, paint_areas, diagnostic_overlays);
    }

    paint_comparison_divider(ui, workspace, state, paint_areas);

    let Some(registration) = &state.manual_registration else {
        return;
    };
    let expected_pane = state.expected_registration_pane();
    for area in paint_areas {
        if expected_pane == Some(area.pane_id) {
            ui.painter().rect_stroke(
                area.rect.shrink(2.0),
                0.0,
                Stroke::new(2.0, Color32::from_rgb(255, 188, 76)),
                StrokeKind::Inside,
            );
        }
    }
    for (sample_index, (pane_id, point)) in registration.samples.iter().enumerate() {
        let Some(area) = paint_areas.iter().find(|area| area.pane_id == *pane_id) else {
            continue;
        };
        let Some(pane) = workspace.panes.iter().find(|pane| pane.id == *pane_id) else {
            continue;
        };
        let Some(position) = normalized_point_to_screen(*point, pane, area) else {
            continue;
        };
        let color = if *pane_id == registration.reference_pane {
            Color32::from_rgb(92, 183, 255)
        } else {
            Color32::from_rgb(99, 221, 161)
        };
        ui.painter()
            .circle_filled(position, 7.0, Color32::from_black_alpha(180));
        ui.painter()
            .circle_stroke(position, 7.0, Stroke::new(2.0, color));
        ui.painter().line_segment(
            [
                position - egui::vec2(10.0, 0.0),
                position + egui::vec2(10.0, 0.0),
            ],
            Stroke::new(1.0, color),
        );
        ui.painter().line_segment(
            [
                position - egui::vec2(0.0, 10.0),
                position + egui::vec2(0.0, 10.0),
            ],
            Stroke::new(1.0, color),
        );
        ui.painter().text(
            position + egui::vec2(10.0, -10.0),
            Align2::LEFT_BOTTOM,
            format!("{}", sample_index / 2 + 1),
            FontId::monospace(11.0),
            Color32::WHITE,
        );
    }
}

fn paint_comparison_divider(
    ui: &Ui,
    workspace: &Workspace,
    state: &UiState,
    paint_areas: &[PanePaintArea],
) {
    let mode = state.comparison_mode();
    let Some((_, target)) = focused_comparison_panes(workspace, state) else {
        return;
    };
    let Some(area) = paint_areas.iter().find(|area| area.pane_id == target) else {
        return;
    };
    if mode == ComparisonMode::Normal {
        if state.blink_reference_visible() {
            let badge = Rect::from_min_size(
                pos2(area.rect.right() - 48.0, area.rect.bottom() - 24.0),
                Vec2::new(42.0, 18.0),
            );
            let painter = ui.painter().with_clip_rect(area.rect);
            painter.rect_filled(badge, 2.0, Color32::from_black_alpha(190));
            painter.text(
                badge.center(),
                Align2::CENTER_CENTER,
                "REF",
                FontId::monospace(10.0),
                Color32::WHITE,
            );
        }
        return;
    }
    let position = state.split_position().clamp(0.02, 0.98);
    let (from, to, handle, reference_rect) = match mode {
        ComparisonMode::Normal => return,
        ComparisonMode::VerticalSplit => {
            let x = egui::lerp(area.rect.left()..=area.rect.right(), position);
            (
                pos2(x, area.rect.top()),
                pos2(x, area.rect.bottom()),
                Rect::from_center_size(pos2(x, area.rect.center().y), Vec2::new(5.0, 32.0)),
                Rect::from_min_max(area.rect.min, pos2(x, area.rect.bottom())),
            )
        }
        ComparisonMode::HorizontalSplit => {
            let y = egui::lerp(area.rect.top()..=area.rect.bottom(), position);
            (
                pos2(area.rect.left(), y),
                pos2(area.rect.right(), y),
                Rect::from_center_size(pos2(area.rect.center().x, y), Vec2::new(32.0, 5.0)),
                Rect::from_min_max(area.rect.min, pos2(area.rect.right(), y)),
            )
        }
    };
    let painter = ui.painter().with_clip_rect(area.rect);
    painter.line_segment([from, to], Stroke::new(3.0, Color32::from_black_alpha(180)));
    painter.line_segment(
        [from, to],
        Stroke::new(1.0, Color32::from_rgb(218, 231, 240)),
    );
    painter.rect_filled(handle, 2.0, Color32::from_black_alpha(190));
    painter.rect_stroke(
        handle,
        2.0,
        Stroke::new(1.0, Color32::from_rgb(218, 231, 240)),
        StrokeKind::Inside,
    );
    let badge = Rect::from_min_size(
        pos2(reference_rect.left() + 6.0, reference_rect.bottom() - 24.0),
        Vec2::new(42.0, 18.0),
    );
    let reference_painter = ui.painter().with_clip_rect(reference_rect);
    reference_painter.rect_filled(badge, 2.0, Color32::from_black_alpha(160));
    reference_painter.text(
        badge.center(),
        Align2::CENTER_CENTER,
        "REF",
        FontId::monospace(10.0),
        Color32::from_gray(224),
    );
}

#[derive(Debug, PartialEq)]
struct PixelGridGeometry {
    image_rect: Rect,
    vertical_lines: Vec<f32>,
    horizontal_lines: Vec<f32>,
}

fn paint_pixel_grids(
    ui: &Ui,
    workspace: &Workspace,
    state: &UiState,
    paint_areas: &[PanePaintArea],
) {
    let color = if ui.visuals().dark_mode {
        Color32::from_rgba_unmultiplied(225, 232, 238, 76)
    } else {
        Color32::from_rgba_unmultiplied(18, 24, 30, 76)
    };
    let comparison = focused_comparison_panes(workspace, state);
    for area in paint_areas {
        let Some(pane) = workspace.panes.iter().find(|pane| pane.id == area.pane_id) else {
            continue;
        };
        if comparison.is_some_and(|(_, target)| target == area.pane_id) {
            let (reference_id, _) = comparison.expect("comparison pair exists");
            let reference = workspace
                .panes
                .iter()
                .find(|candidate| candidate.id == reference_id)
                .expect("comparison reference exists");
            match state.comparison_mode() {
                ComparisonMode::Normal if state.blink_reference_visible() => {
                    paint_pixel_grid(ui, reference, area, area.rect, color);
                }
                ComparisonMode::VerticalSplit => {
                    let x = egui::lerp(
                        area.rect.left()..=area.rect.right(),
                        state.split_position().clamp(0.02, 0.98),
                    );
                    paint_pixel_grid(
                        ui,
                        reference,
                        area,
                        Rect::from_min_max(area.rect.min, pos2(x, area.rect.bottom())),
                        color,
                    );
                    paint_pixel_grid(
                        ui,
                        pane,
                        area,
                        Rect::from_min_max(pos2(x, area.rect.top()), area.rect.max),
                        color,
                    );
                }
                ComparisonMode::HorizontalSplit => {
                    let y = egui::lerp(
                        area.rect.top()..=area.rect.bottom(),
                        state.split_position().clamp(0.02, 0.98),
                    );
                    paint_pixel_grid(
                        ui,
                        reference,
                        area,
                        Rect::from_min_max(area.rect.min, pos2(area.rect.right(), y)),
                        color,
                    );
                    paint_pixel_grid(
                        ui,
                        pane,
                        area,
                        Rect::from_min_max(pos2(area.rect.left(), y), area.rect.max),
                        color,
                    );
                }
                ComparisonMode::Normal => {
                    paint_pixel_grid(ui, pane, area, area.rect, color);
                }
            }
        } else {
            paint_pixel_grid(ui, pane, area, area.rect, color);
        }
    }
}

fn paint_pixel_grid(
    ui: &Ui,
    pane: &viewer_model::Pane,
    area: &PanePaintArea,
    comparison_clip: Rect,
    color: Color32,
) {
    let Some(grid) = pixel_grid_geometry(pane, area) else {
        return;
    };
    let clip = grid.image_rect.intersect(comparison_clip);
    if !clip.is_positive() {
        return;
    }
    let pixels_per_point = (area.physical_size[0] / area.rect.width().max(1.0)).max(0.01);
    let painter = ui.painter().with_clip_rect(clip);
    let stroke = Stroke::new((1.0 / pixels_per_point).max(0.5), color);
    for x in grid.vertical_lines {
        painter.line_segment(
            [
                pos2(x, grid.image_rect.top()),
                pos2(x, grid.image_rect.bottom()),
            ],
            stroke,
        );
    }
    for y in grid.horizontal_lines {
        painter.line_segment(
            [
                pos2(grid.image_rect.left(), y),
                pos2(grid.image_rect.right(), y),
            ],
            stroke,
        );
    }
}

fn pixel_grid_geometry(
    pane: &viewer_model::Pane,
    area: &PanePaintArea,
) -> Option<PixelGridGeometry> {
    let [source_width, source_height] = pane.image_size?;
    let source_scale = pane.viewport.source_pixels_per_physical_pixel;
    if !source_scale.is_finite()
        || source_scale <= 0.0
        || source_scale.recip() + f64::EPSILON < MIN_PIXEL_GRID_SIZE_PHYSICAL
    {
        return None;
    }
    let pixels_per_point_x =
        f64::from(area.physical_size[0]) / f64::from(area.rect.width().max(1.0));
    let pixels_per_point_y =
        f64::from(area.physical_size[1]) / f64::from(area.rect.height().max(1.0));
    if !pixels_per_point_x.is_finite()
        || !pixels_per_point_y.is_finite()
        || pixels_per_point_x <= 0.0
        || pixels_per_point_y <= 0.0
    {
        return None;
    }

    let vertical_lines = pixel_grid_axis_lines(
        pane.viewport.center.x,
        source_width,
        source_scale,
        area.rect.center().x,
        pixels_per_point_x,
        area.physical_size[0],
    )?;
    let horizontal_lines = pixel_grid_axis_lines(
        pane.viewport.center.y,
        source_height,
        source_scale,
        area.rect.center().y,
        pixels_per_point_y,
        area.physical_size[1],
    )?;
    let source_left = source_boundary_to_screen(
        0.0,
        pane.viewport.center.x,
        source_width,
        source_scale,
        area.rect.center().x,
        pixels_per_point_x,
    );
    let source_right = source_boundary_to_screen(
        f64::from(source_width),
        pane.viewport.center.x,
        source_width,
        source_scale,
        area.rect.center().x,
        pixels_per_point_x,
    );
    let source_top = source_boundary_to_screen(
        0.0,
        pane.viewport.center.y,
        source_height,
        source_scale,
        area.rect.center().y,
        pixels_per_point_y,
    );
    let source_bottom = source_boundary_to_screen(
        f64::from(source_height),
        pane.viewport.center.y,
        source_height,
        source_scale,
        area.rect.center().y,
        pixels_per_point_y,
    );
    let image_rect = Rect::from_min_max(
        pos2(
            source_left.max(area.rect.left()),
            source_top.max(area.rect.top()),
        ),
        pos2(
            source_right.min(area.rect.right()),
            source_bottom.min(area.rect.bottom()),
        ),
    );
    (!image_rect.is_negative()).then_some(PixelGridGeometry {
        image_rect,
        vertical_lines,
        horizontal_lines,
    })
}

fn pixel_grid_axis_lines(
    center_normalized: f64,
    source_extent: u32,
    source_scale: f64,
    screen_center: f32,
    pixels_per_point: f64,
    physical_extent: f32,
) -> Option<Vec<f32>> {
    let center_source = center_normalized * f64::from(source_extent);
    let half_visible_source = f64::from(physical_extent) * source_scale * 0.5;
    let first = (center_source - half_visible_source).ceil().max(0.0) as u32;
    let last = (center_source + half_visible_source)
        .floor()
        .min(f64::from(source_extent)) as u32;
    if last < first {
        return Some(Vec::new());
    }
    let line_count = last as usize - first as usize + 1;
    if line_count > MAX_PIXEL_GRID_LINES_PER_AXIS {
        return None;
    }
    Some(
        (first..=last)
            .map(|boundary| {
                source_boundary_to_screen(
                    f64::from(boundary),
                    center_normalized,
                    source_extent,
                    source_scale,
                    screen_center,
                    pixels_per_point,
                )
            })
            .collect(),
    )
}

fn source_boundary_to_screen(
    boundary: f64,
    center_normalized: f64,
    source_extent: u32,
    source_scale: f64,
    screen_center: f32,
    pixels_per_point: f64,
) -> f32 {
    screen_center
        + ((boundary - center_normalized * f64::from(source_extent))
            / source_scale
            / pixels_per_point) as f32
}

fn paint_automatic_alignment_diagnostics(
    ui: &Ui,
    workspace: &Workspace,
    paint_areas: &[PanePaintArea],
    diagnostic_overlays: &[AlignmentDiagnosticOverlay],
) {
    for overlay in diagnostic_overlays {
        let Some(reference_area) = paint_areas
            .iter()
            .find(|area| area.pane_id == overlay.reference_pane)
        else {
            continue;
        };
        let Some(target_area) = paint_areas
            .iter()
            .find(|area| area.pane_id == overlay.target_pane)
        else {
            continue;
        };
        let Some(reference_pane) = workspace
            .panes
            .iter()
            .find(|pane| pane.id == overlay.reference_pane)
        else {
            continue;
        };
        let Some(target_pane) = workspace
            .panes
            .iter()
            .find(|pane| pane.id == overlay.target_pane)
        else {
            continue;
        };

        for feature_match in &overlay.matches {
            let reference_position =
                normalized_point_to_screen(feature_match.reference, reference_pane, reference_area);
            let target_position =
                normalized_point_to_screen(feature_match.target, target_pane, target_area);
            let (point_color, line_color) = if feature_match.inlier {
                (
                    Color32::from_rgb(74, 222, 144),
                    Color32::from_rgba_unmultiplied(74, 222, 144, 62),
                )
            } else {
                (
                    Color32::from_rgb(255, 164, 72),
                    Color32::from_rgba_unmultiplied(255, 164, 72, 42),
                )
            };
            if let (Some(reference_position), Some(target_position)) =
                (reference_position, target_position)
            {
                ui.painter().line_segment(
                    [reference_position, target_position],
                    Stroke::new(0.75, line_color),
                );
            }
            for position in [reference_position, target_position].into_iter().flatten() {
                ui.painter()
                    .circle_filled(position, 3.5, Color32::from_black_alpha(160));
                ui.painter()
                    .circle_stroke(position, 3.5, Stroke::new(1.5, point_color));
            }
        }
    }
}

fn normalized_point_to_screen(
    point: NormalizedPoint,
    pane: &viewer_model::Pane,
    area: &PanePaintArea,
) -> Option<egui::Pos2> {
    let [image_width, image_height] = pane.image_size?;
    let pixels_per_point = (area.physical_size[0] / area.rect.width().max(1.0)).max(0.01);
    let offset_physical = egui::vec2(
        ((point.x - pane.viewport.center.x) * f64::from(image_width)
            / pane
                .viewport
                .source_pixels_per_physical_pixel
                .max(viewer_model::Viewport::MIN_SCALE)) as f32,
        ((point.y - pane.viewport.center.y) * f64::from(image_height)
            / pane
                .viewport
                .source_pixels_per_physical_pixel
                .max(viewer_model::Viewport::MIN_SCALE)) as f32,
    );
    let position = area.rect.center() + offset_physical / pixels_per_point;
    area.rect.contains(position).then_some(position)
}

fn rect_to_physical(rect: Rect, pixels_per_point: f32) -> [f32; 4] {
    [
        rect.left() * pixels_per_point,
        rect.top() * pixels_per_point,
        rect.width() * pixels_per_point,
        rect.height() * pixels_per_point,
    ]
}

fn toolbar_group_separator(ui: &mut Ui) {
    ui.add_space(2.0);
    ui.separator();
    ui.add_space(2.0);
}

const fn sync_mode_short_label(mode: SyncMode) -> &'static str {
    match mode {
        SyncMode::FitRelative => "Fit-relative",
        SyncMode::WidthRelative => "Width-relative",
        SyncMode::HeightRelative => "Height-relative",
        SyncMode::SourcePixels => "Source pixels",
    }
}

fn draw_toolbar(ui: &mut Ui, workspace: &mut Workspace, state: &mut UiState) -> ToolbarOutput {
    let mut output = ToolbarOutput::default();
    let has_any_image = workspace.panes.iter().any(|pane| pane.image_id.is_some());
    let active_has_image = workspace.active_pane.is_some_and(|active| {
        workspace
            .panes
            .iter()
            .any(|pane| pane.id == active && pane.image_id.is_some())
    });
    if let Some(focused_pane) = state.focused_pane {
        ui.horizontal_centered(|ui| {
            draw_focused_toolbar(ui, workspace, state, focused_pane, &mut output);
        });
        return output;
    }
    ui.horizontal_centered(|ui| {
        ui.add_space(TOOLBAR_MARGIN);
        if ui
            .button("Open…")
            .on_hover_text("Open up to eight JPEG or supported RAW files")
            .clicked()
        {
            output.open_requested = true;
        }
        if ui
            .add_enabled(
                workspace.panes.len() < MAX_PANES,
                egui::Button::new("Add pane"),
            )
            .on_hover_text("Add a comparison pane; close individual panes with − in their headers")
            .clicked()
            && let Ok(pane_id) = workspace.add_pane()
        {
            state.focused_pane = None;
            output.added_panes.push(pane_id);
        }
        ui.menu_button("Layout", |ui| {
            for mode in LayoutMode::ALL {
                ui.selectable_value(&mut workspace.layout_mode, mode, mode.label());
            }
        });
        toolbar_group_separator(ui);
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            if ui
                .add_enabled(has_any_image, egui::Button::new("Fit"))
                .on_hover_text("Fit all images to their panes")
                .clicked()
            {
                workspace.fit_all();
            }
            if ui
                .add_enabled(has_any_image, egui::Button::new("1:1"))
                .on_hover_text("One source pixel per physical screen pixel")
                .clicked()
            {
                workspace.one_to_one_all();
                output.view_one_to_one_requested = true;
            }
        });
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let mut synchronized = workspace.synchronized;
            if ui.checkbox(&mut synchronized, "Sync").changed() {
                workspace.set_synchronized(synchronized);
            }
            ui.menu_button(sync_mode_short_label(workspace.sync_mode), |ui| {
                ui.strong("Synchronized navigation");
                for mode in SyncMode::ALL {
                    ui.selectable_value(&mut workspace.sync_mode, mode, mode.label());
                }
                ui.separator();
                if ui.button("Reset alignment").clicked() {
                    workspace.reset_sync_adjustments();
                    ui.close();
                }
            });
        });
        toolbar_group_separator(ui);
        let reference_pane = workspace.reference_pane_id();
        let active_pane = workspace.active_pane;
        let reference_has_image = reference_pane.is_some_and(|reference| {
            workspace
                .panes
                .iter()
                .any(|pane| pane.id == reference && pane.image_id.is_some())
        });
        let active_is_target = active_pane.is_some_and(|active| {
            active != reference_pane.unwrap_or(active)
                && workspace
                    .panes
                    .iter()
                    .any(|pane| pane.id == active && pane.image_id.is_some())
        });
        ui.menu_button(
            if state.registration_busy {
                "Aligning…"
            } else {
                "Align"
            },
            |ui| {
                if state.registration_busy {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Matching image features");
                    });
                    ui.separator();
                }
                if let Some((reference, target, sample_count)) =
                    state.manual_registration.as_ref().map(|registration| {
                        (
                            registration.reference_pane,
                            registration.target_pane,
                            registration.samples.len(),
                        )
                    })
                {
                    let expected = if sample_count % 2 == 0 {
                        reference
                    } else {
                        target
                    };
                    let point_number = sample_count / 2 + 1;
                    let expected_title = workspace
                        .panes
                        .iter()
                        .find(|pane| pane.id == expected)
                        .map_or("pane", |pane| pane.title.as_str());
                    ui.strong("Manual alignment");
                    ui.label(format!("Click point {point_number} in {expected_title}"));
                    ui.weak("Drag to pan or scroll to zoom before clicking.");
                    if ui.button("Cancel manual alignment").clicked() {
                        state.manual_registration = None;
                        ui.close();
                    }
                    return;
                }

                let reference_title = reference_pane
                    .and_then(|reference| {
                        workspace
                            .panes
                            .iter()
                            .find(|pane| pane.id == reference)
                            .map(|pane| pane.title.as_str())
                    })
                    .unwrap_or("None");
                ui.label(format!("Reference: {reference_title}"));
                if ui
                    .add_enabled(
                        active_has_image && active_pane != reference_pane,
                        egui::Button::new("Set active as reference"),
                    )
                    .clicked()
                    && let Some(active) = active_pane
                {
                    output.registration_request = Some(RegistrationRequest::SetReference(active));
                    ui.close();
                }
                ui.separator();
                if ui
                    .add_enabled(
                        reference_has_image && active_is_target && !state.registration_busy,
                        egui::Button::new("Auto align active to reference"),
                    )
                    .clicked()
                    && let Some(active) = active_pane
                {
                    output.registration_request = Some(RegistrationRequest::Automatic(active));
                    ui.close();
                }
                if ui
                    .add_enabled(
                        reference_has_image
                            && workspace
                                .panes
                                .iter()
                                .filter(|pane| pane.image_id.is_some())
                                .count()
                                > 1
                            && !state.registration_busy,
                        egui::Button::new("Auto align all to reference"),
                    )
                    .clicked()
                {
                    output.registration_request = Some(RegistrationRequest::AutomaticAll);
                    ui.close();
                }
                if ui
                    .add_enabled(
                        reference_has_image && active_is_target && !state.registration_busy,
                        egui::Button::new("Manual alignment…"),
                    )
                    .clicked()
                    && let (Some(reference), Some(target)) = (reference_pane, active_pane)
                {
                    state.start_manual_registration(reference, target);
                    ui.close();
                }
                ui.separator();
                if ui
                    .add_enabled(
                        active_is_target,
                        egui::Button::new("Reset active alignment"),
                    )
                    .clicked()
                    && let Some(active) = active_pane
                {
                    output.registration_request = Some(RegistrationRequest::Reset(active));
                    ui.close();
                }
                if ui.button("Reset all alignments").clicked() {
                    output.registration_request = Some(RegistrationRequest::ResetAll);
                    ui.close();
                }
                if let Some(quality) = &state.alignment_quality {
                    ui.separator();
                    ui.strong(format!("Last auto align · pane {}", quality.target_pane.0));
                    ui.label(format!(
                        "Features: {} reference · {} target",
                        quality.reference_features, quality.target_features
                    ));
                    ui.label(format!(
                        "Matches: {} candidates · {} inliers",
                        quality.candidate_matches, quality.inliers
                    ));
                    if quality.succeeded {
                        let confidence = quality.confidence.unwrap_or_default() * 100.0;
                        let error = quality.median_error_pixels.unwrap_or_default();
                        ui.label(format!(
                            "Confidence: {confidence:.0}% · median error {error:.1}px"
                        ));
                    }
                    ui.checkbox(
                        &mut state.show_alignment_diagnostics,
                        "Show match diagnostics",
                    )
                    .on_hover_text("Overlay accepted and rejected feature matches on the images");
                    if state.show_alignment_diagnostics {
                        ui.weak("Green = accepted · orange = rejected");
                    }
                }
                if let Some(status) = &state.registration_status {
                    ui.separator();
                    ui.weak(status);
                }
            },
        );
        toolbar_group_separator(ui);
        ui.menu_button("RAW", |ui| {
            ui.strong("Full-resolution RAW");
            if ui
                .checkbox(
                    &mut state.match_raw_to_preview,
                    "Match embedded JPEG automatically",
                )
                .on_hover_text(
                    "Apply an instant, non-destructive GPU tone match whenever a full RAW loads",
                )
                .changed()
            {
                output.preview_match_enabled_changed = Some(state.match_raw_to_preview);
            }
            ui.checkbox(&mut state.develop_raws_on_load, "Develop on load")
                .on_hover_text(
                    "Otherwise Frank develops when zoom exceeds the embedded preview detail",
                );
            ui.separator();
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(state.active_is_raw, egui::Button::new("Develop active RAW"))
                    .clicked()
                {
                    output.raw_develop_requested = true;
                    ui.close();
                }
                if ui
                    .add_enabled(state.has_raw_images, egui::Button::new("Develop all RAWs"))
                    .clicked()
                {
                    output.raw_develop_all_requested = true;
                    ui.close();
                }
            });
            ui.add_enabled_ui(state.active_is_raw, |ui| {
                if state.match_raw_to_preview && ui.button("Re-match active to preview").clicked() {
                    output.preview_match_requested = true;
                    ui.close();
                }
            });
        });
        ui.add_enabled_ui(active_has_image, |ui| {
            ui.menu_button("Exposure", |ui| {
                let active_index = workspace
                    .active_pane
                    .and_then(|active| workspace.panes.iter().position(|pane| pane.id == active));
                if let Some(active_index) = active_index {
                    ui.strong("Active pane exposure");
                    let mut exposure = workspace.panes[active_index].manual_exposure_ev;
                    let changed = ui
                        .add(egui::Slider::new(&mut exposure, -4.0..=4.0).text("EV"))
                        .changed();
                    if changed {
                        workspace.panes[active_index].manual_exposure_ev = exposure;
                        if state.sync_adjustments {
                            for pane in &mut workspace.panes {
                                if pane.linked {
                                    pane.manual_exposure_ev = exposure;
                                }
                            }
                        }
                    }
                    ui.checkbox(&mut state.sync_adjustments, "Apply to linked panes");
                    ui.separator();
                    if ui.button("Normalize visible views to reference").clicked() {
                        output.exposure_match_requested = true;
                        ui.close();
                    }
                    if ui.button("Clear normalization").clicked() {
                        output.exposure_match_reset_requested = true;
                        ui.close();
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Reset active").clicked() {
                            workspace.panes[active_index].manual_exposure_ev = 0.0;
                        }
                        if ui.button("Reset all").clicked() {
                            for pane in &mut workspace.panes {
                                pane.manual_exposure_ev = 0.0;
                            }
                        }
                    });
                } else {
                    ui.weak("Select a pane first");
                }
            });
        });
        toolbar_group_separator(ui);
        let mut clean_view = !state.show_pane_controls;
        if ui
            .checkbox(&mut clean_view, "Clean view")
            .on_hover_text("Hide pane headers for borderless comparison")
            .changed()
        {
            state.show_pane_controls = !clean_view;
        }
        ui.menu_button("...", |ui| {
            ui.strong("Theme");
            ui.horizontal(|ui| {
                ui.selectable_value(&mut state.theme, AppTheme::Light, "Light");
                ui.selectable_value(&mut state.theme, AppTheme::Dark, "Dark");
                ui.selectable_value(&mut state.theme, AppTheme::System, "System");
            });
            ui.separator();
            ui.strong("Pane title fields");
            ui.label("Filename is always shown");
            ui.separator();
            ui.checkbox(&mut workspace.title_fields.megapixels, "Megapixels");
            ui.checkbox(&mut workspace.title_fields.camera, "Camera");
            ui.checkbox(&mut workspace.title_fields.lens, "Lens");
            ui.checkbox(&mut workspace.title_fields.bit_depth, "Bit depth");
            ui.separator();
            ui.checkbox(&mut workspace.title_fields.iso, "ISO");
            ui.checkbox(&mut workspace.title_fields.shutter, "Shutter");
            ui.checkbox(&mut workspace.title_fields.aperture, "Aperture");
            ui.checkbox(&mut workspace.title_fields.focal_length, "Focal length");
            ui.checkbox(&mut workspace.title_fields.quality, "Preview quality");
            ui.separator();
            if ui.button("Restore defaults").clicked() {
                workspace.title_fields = TitleFields::default();
            }
            ui.separator();
            ui.checkbox(&mut state.show_pixel_grid, "Pixel grid")
                .on_hover_text("Show source-pixel boundaries at 600% magnification and closer");
        })
        .response
        .on_hover_text("Display and title settings");
    });
    output
}

fn draw_focused_toolbar(
    ui: &mut Ui,
    workspace: &mut Workspace,
    state: &mut UiState,
    focused_pane: PaneId,
    output: &mut ToolbarOutput,
) {
    ui.add_space(TOOLBAR_MARGIN);
    if ui
        .button("All panes")
        .on_hover_text("Return to the comparison grid · F or Esc")
        .clicked()
    {
        state.focused_pane = None;
        return;
    }

    let focused_title = workspace
        .panes
        .iter()
        .find(|pane| pane.id == focused_pane)
        .map_or("Image", |pane| pane.title.as_str());
    let comparison = focused_comparison_panes(workspace, state);
    let reference_title = comparison.and_then(|(reference, _)| {
        workspace
            .panes
            .iter()
            .find(|pane| pane.id == reference)
            .map(|pane| pane.title.as_str())
    });
    let (full_caption, compact_caption) = if let Some(reference_title) = reference_title {
        (
            format!("{focused_title}  vs  REF {reference_title}"),
            format!(
                "{}  vs  REF {}",
                compact_label(focused_title, 18),
                compact_label(reference_title, 14)
            ),
        )
    } else {
        (focused_title.to_owned(), compact_label(focused_title, 34))
    };
    ui.label(compact_caption).on_hover_text(full_caption);
    toolbar_group_separator(ui);

    ui.add_enabled_ui(comparison.is_some(), |ui| {
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let target_selected =
                state.comparison_mode == ComparisonMode::Normal && !state.blink_reference_visible();
            if ui.selectable_label(target_selected, "Target").clicked() {
                state.comparison_mode = ComparisonMode::Normal;
                state.blink_latched = false;
            }
            let reference_selected =
                state.comparison_mode == ComparisonMode::Normal && state.blink_reference_visible();
            if ui
                .selectable_label(reference_selected, "Reference")
                .on_hover_text("Hold Space for a temporary reference blink")
                .clicked()
            {
                state.comparison_mode = ComparisonMode::Normal;
                state.blink_latched = true;
            }
            if ui
                .selectable_label(
                    state.comparison_mode == ComparisonMode::VerticalSplit,
                    "Left/right",
                )
                .on_hover_text("Reference on the left · target on the right")
                .clicked()
            {
                state.comparison_mode = ComparisonMode::VerticalSplit;
                state.blink_latched = false;
            }
            if ui
                .selectable_label(
                    state.comparison_mode == ComparisonMode::HorizontalSplit,
                    "Top/bottom",
                )
                .on_hover_text("Reference on top · target on the bottom")
                .clicked()
            {
                state.comparison_mode = ComparisonMode::HorizontalSplit;
                state.blink_latched = false;
            }
        });
    });
    if ui
        .add_enabled(
            state.comparison_mode.is_split(),
            egui::Button::new("Center"),
        )
        .on_hover_text("Center the split divider; double-clicking it does the same")
        .clicked()
    {
        state.split_position = 0.5;
    }
    toolbar_group_separator(ui);

    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        if ui
            .button("Fit")
            .on_hover_text("Fit the comparison to the focused viewport")
            .clicked()
        {
            workspace.fit_all();
        }
        if ui
            .button("1:1")
            .on_hover_text("One source pixel per physical screen pixel")
            .clicked()
        {
            workspace.one_to_one_all();
            output.view_one_to_one_requested = true;
        }
    });
    let mut synchronized = workspace.synchronized;
    if ui.checkbox(&mut synchronized, "Sync").changed() {
        workspace.set_synchronized(synchronized);
    }
    let mut clean_view = !state.show_pane_controls;
    if ui
        .checkbox(&mut clean_view, "Clean")
        .on_hover_text("Hide the focused pane header")
        .changed()
    {
        state.show_pane_controls = !clean_view;
    }
}

fn compact_label(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars {
        return text.to_owned();
    }
    let visible = max_chars.saturating_sub(3);
    format!("{}...", text.chars().take(visible).collect::<String>())
}

fn draw_pane(
    ui: &mut Ui,
    workspace: &mut Workspace,
    state: &mut UiState,
    pane_id: PaneId,
    geometry: PaneDrawGeometry,
    output: &mut ToolbarOutput,
) {
    let PaneDrawGeometry {
        rect: pane_rect,
        pixels_per_point,
        colors,
    } = geometry;
    let is_active = workspace.active_pane == Some(pane_id);
    let reference_pane = workspace.reference_pane_id();
    let is_reference = reference_pane == Some(pane_id);
    let title_height = if state.show_pane_controls {
        TITLE_HEIGHT
    } else {
        0.0
    };
    let title_rect = Rect::from_min_max(
        pane_rect.min,
        pos2(pane_rect.right(), pane_rect.top() + title_height),
    );
    let image_rect = Rect::from_min_max(
        pos2(pane_rect.left(), title_rect.bottom()),
        pane_rect.right_bottom(),
    );
    let close_rect = Rect::from_min_max(
        pos2(title_rect.right() - CLOSE_CONTROL_WIDTH, title_rect.top()),
        title_rect.right_bottom(),
    )
    .shrink2(Vec2::new(4.0, 7.0));
    let focus_rect = Rect::from_min_max(
        pos2(close_rect.left() - FOCUS_CONTROL_WIDTH, title_rect.top()),
        pos2(close_rect.left() - 2.0, title_rect.bottom()),
    )
    .shrink2(Vec2::new(3.0, 7.0));
    let link_rect = Rect::from_min_max(
        pos2(focus_rect.left() - LINK_CONTROL_WIDTH, title_rect.top()),
        pos2(focus_rect.left() - 2.0, title_rect.bottom()),
    )
    .shrink2(Vec2::new(2.0, 7.0));
    let note_rect = Rect::from_min_max(
        pos2(link_rect.left() - NOTE_CONTROL_WIDTH, title_rect.top()),
        pos2(link_rect.left() - 2.0, title_rect.bottom()),
    )
    .shrink2(Vec2::new(3.0, 7.0));
    let reference_slot = Rect::from_min_max(
        title_rect.min,
        pos2(
            title_rect.left() + REFERENCE_CONTROL_WIDTH,
            title_rect.bottom(),
        ),
    );
    let reference_rect = reference_slot.shrink2(Vec2::new(5.0, 7.0));
    let drag_rect = Rect::from_min_max(
        pos2(reference_slot.right(), title_rect.top()),
        pos2(note_rect.left() - 2.0, title_rect.bottom()),
    );

    ui.painter().rect_filled(pane_rect, 0.0, colors.pane);
    if state.show_pane_controls {
        ui.painter().rect_filled(title_rect, 0.0, colors.header);
    }
    ui.painter().rect_filled(image_rect, 0.0, colors.image);
    if state.show_pane_controls {
        ui.painter().line_segment(
            [title_rect.left_bottom(), title_rect.right_bottom()],
            Stroke::new(1.0, colors.header_border),
        );
    }
    if is_active && state.show_pane_controls {
        ui.painter().rect_filled(
            Rect::from_min_max(
                pane_rect.min,
                pos2(pane_rect.right(), pane_rect.top() + 2.0),
            ),
            0.0,
            colors.active,
        );
    }

    let title_fields = workspace.title_fields;
    let Some(pane_index) = workspace.panes.iter().position(|pane| pane.id == pane_id) else {
        return;
    };
    let reference_metadata = reference_pane
        .filter(|reference| *reference != pane_id)
        .and_then(|reference| {
            workspace
                .panes
                .iter()
                .find(|pane| pane.id == reference)
                .map(|pane| pane.metadata.clone())
        });
    let pane = &workspace.panes[pane_index];
    let image_size = pane.image_size;
    let viewport = pane.viewport;
    let has_image = pane.image_id.is_some();
    let linked = pane.linked;
    let note = pane.note.clone();
    let pane_title = pane.title.clone();
    let zoom_percent = pane.viewport.pixel_zoom_percent();
    let formatted_title = pane.formatted_title_relative(title_fields, reference_metadata.as_ref());
    let metadata = formatted_title
        .strip_prefix(&pane_title)
        .unwrap_or(&formatted_title)
        .trim_start_matches(" · ");
    let secondary = if note.is_empty() {
        metadata.to_owned()
    } else if metadata.is_empty() {
        note.clone()
    } else {
        format!("{metadata}  •  {note}")
    };
    if state.show_pane_controls {
        let zoom_rect = Rect::from_min_max(
            pos2(drag_rect.right() - 62.0, drag_rect.top()),
            drag_rect.right_bottom(),
        );
        let name_rect = Rect::from_min_max(
            drag_rect.min,
            pos2(zoom_rect.left() - 4.0, drag_rect.center().y),
        );
        let reference_response = ui
            .interact(
                reference_rect,
                ui.id().with(("pane-reference", pane_id.0)),
                Sense::click(),
            )
            .on_hover_cursor(CursorIcon::PointingHand)
            .on_hover_text(if !has_image {
                "Load an image before setting this pane as reference"
            } else if is_reference {
                "Reference pane"
            } else {
                "Set as reference · R sets the active pane"
            });
        let reference_fill = if is_reference {
            colors.active
        } else if reference_response.hovered() && has_image {
            colors.note_hover
        } else {
            colors.header
        };
        ui.painter()
            .rect_filled(reference_rect, 2.0, reference_fill);
        ui.painter().rect_stroke(
            reference_rect,
            2.0,
            Stroke::new(
                1.0,
                if is_reference {
                    colors.active
                } else {
                    colors.header_border
                },
            ),
            StrokeKind::Inside,
        );
        ui.painter().text(
            reference_rect.center(),
            Align2::CENTER_CENTER,
            "REF",
            FontId::monospace(9.0),
            if is_reference {
                Color32::WHITE
            } else if has_image {
                colors.secondary_text
            } else {
                colors.placeholder_text
            },
        );
        if reference_response.clicked() && has_image {
            let _ = workspace.set_active(pane_id);
            if !is_reference {
                output.registration_request = Some(RegistrationRequest::SetReference(pane_id));
            }
        }
        ui.painter()
            .with_clip_rect(name_rect.shrink2(Vec2::new(8.0, 0.0)))
            .text(
                pos2(drag_rect.left() + 8.0, title_rect.top() + 13.0),
                Align2::LEFT_CENTER,
                &pane_title,
                FontId::proportional(12.0),
                colors.primary_text,
            );
        if has_image {
            ui.painter().with_clip_rect(zoom_rect).text(
                pos2(zoom_rect.right() - 5.0, title_rect.top() + 13.0),
                Align2::RIGHT_CENTER,
                format!("{zoom_percent:.0}%"),
                FontId::monospace(11.0),
                colors.zoom_text,
            );
        }
        if !secondary.is_empty() {
            ui.painter()
                .with_clip_rect(drag_rect.shrink2(Vec2::new(8.0, 0.0)))
                .text(
                    pos2(drag_rect.left() + 8.0, title_rect.bottom() - 11.0),
                    Align2::LEFT_CENTER,
                    secondary,
                    FontId::proportional(10.5),
                    colors.secondary_text,
                );
        }

        let link_color = if linked {
            colors.link_on
        } else {
            colors.link_off
        };
        ui.painter().rect_filled(link_rect, 2.0, link_color);
        ui.painter().text(
            link_rect.center(),
            Align2::CENTER_CENTER,
            if linked { "SYNC" } else { "FREE" },
            FontId::monospace(9.5),
            if linked {
                Color32::WHITE
            } else {
                colors.primary_text
            },
        );
        let link_response = ui
            .interact(
                link_rect,
                ui.id().with(("pane-link", pane_id.0)),
                Sense::click(),
            )
            .on_hover_cursor(CursorIcon::PointingHand)
            .on_hover_text(if linked {
                "Remove this pane from synchronized pan and zoom"
            } else {
                "Add this pane to synchronized pan and zoom"
            });
        if link_response.clicked() {
            let _ = workspace.toggle_pane_linked(pane_id);
        }

        let pane_is_focused = state.focused_pane == Some(pane_id);
        let focus_response = ui
            .interact(
                focus_rect,
                ui.id().with(("pane-focus", pane_id.0)),
                Sense::click(),
            )
            .on_hover_cursor(CursorIcon::PointingHand)
            .on_hover_text(if pane_is_focused {
                "Return to all panes · F or Esc"
            } else if has_image {
                "Maximize this pane · F or double-click the image"
            } else {
                "Load an image before maximizing this pane"
            });
        if focus_response.hovered() && (has_image || pane_is_focused) {
            ui.painter().rect_filled(focus_rect, 2.0, colors.note_hover);
        }
        paint_focus_icon(
            ui.painter(),
            focus_rect,
            pane_is_focused,
            if has_image || pane_is_focused {
                colors.primary_text
            } else {
                colors.placeholder_text
            },
        );
        if focus_response.clicked() && (has_image || pane_is_focused) {
            if pane_is_focused {
                state.focused_pane = None;
            } else {
                let _ = workspace.set_active(pane_id);
                maximize_pane(state, pane_id);
            }
        }

        let close_response = ui
            .interact(
                close_rect,
                ui.id().with(("pane-close", pane_id.0)),
                Sense::click(),
            )
            .on_hover_cursor(CursorIcon::PointingHand)
            .on_hover_text("Close this pane");
        ui.painter().text(
            close_rect.center(),
            Align2::CENTER_CENTER,
            "−",
            FontId::proportional(18.0),
            if close_response.hovered() {
                colors.primary_text
            } else {
                colors.secondary_text
            },
        );
        if close_response.clicked() && workspace.panes.len() > MIN_PANES {
            output.removed_panes.push(pane_id);
        }

        let note_response = ui
            .interact(
                note_rect,
                ui.id().with(("pane-note", pane_id.0)),
                Sense::click(),
            )
            .on_hover_cursor(CursorIcon::PointingHand)
            .on_hover_text(if note.is_empty() {
                "Add image note"
            } else {
                "Edit image note"
            });
        let note_background = if note_response.hovered() {
            colors.note_hover
        } else if note.is_empty() {
            colors.note_empty
        } else {
            colors.note_filled
        };
        ui.painter().rect_filled(note_rect, 2.0, note_background);
        let icon_color = if note.is_empty() {
            colors.primary_text
        } else {
            Color32::WHITE
        };
        let center = note_rect.center();
        ui.painter().line_segment(
            [
                pos2(center.x - 4.0, center.y + 4.0),
                pos2(center.x + 3.5, center.y - 3.5),
            ],
            Stroke::new(2.2, icon_color),
        );
        ui.painter().line_segment(
            [
                pos2(center.x + 1.5, center.y - 5.0),
                pos2(center.x + 5.0, center.y - 1.5),
            ],
            Stroke::new(1.8, icon_color),
        );
        ui.painter().line_segment(
            [
                pos2(center.x - 5.0, center.y + 5.0),
                pos2(center.x - 4.2, center.y + 2.1),
            ],
            Stroke::new(1.4, icon_color),
        );
        if note_response.clicked() {
            begin_note_edit(state, pane_id, note.clone());
        }

        let title_response = ui
            .interact(
                drag_rect,
                ui.id().with(("pane-title", pane_id.0)),
                Sense::click_and_drag(),
            )
            .on_hover_cursor(if state.dragged_pane.is_some() {
                CursorIcon::Grabbing
            } else {
                CursorIcon::Grab
            })
            .on_hover_text("Drag to reorder · double-click to edit the image note");
        if title_response.clicked() || title_response.drag_started() {
            let _ = workspace.set_active(pane_id);
        }
        if title_response.drag_started() {
            state.dragged_pane = Some(pane_id);
            state.drop_target = Some(pane_id);
        }
        if title_response.double_clicked() {
            begin_note_edit(state, pane_id, note.clone());
        }
        let pointer_over_title = ui.input(|input| {
            input
                .pointer
                .hover_pos()
                .is_some_and(|pointer| drag_rect.contains(pointer))
        });
        if state.dragged_pane.is_some() && pointer_over_title {
            state.drop_target = Some(pane_id);
        }
        if state.dragged_pane == Some(pane_id) {
            ui.painter().rect_stroke(
                title_rect.shrink(2.0),
                3.0,
                Stroke::new(2.0, Color32::from_rgb(94, 174, 255)),
                StrokeKind::Inside,
            );
        } else if state.drop_target == Some(pane_id) && state.dragged_pane.is_some() {
            ui.painter().rect_stroke(
                pane_rect.shrink(2.0),
                4.0,
                Stroke::new(3.0, Color32::from_rgb(100, 220, 170)),
                StrokeKind::Inside,
            );
        }
    }
    if !has_image {
        ui.painter().text(
            image_rect.center(),
            Align2::CENTER_CENTER,
            "Drop an image here\nJPEG  ·  RAW",
            FontId::proportional(14.0),
            colors.placeholder_text,
        );
    }

    let response = ui.interact(
        image_rect,
        ui.id().with(("pane", pane_id.0)),
        Sense::click_and_drag(),
    );
    if response.secondary_clicked() {
        let _ = workspace.set_active(pane_id);
    }
    response.context_menu(|ui| {
        ui.strong(&pane_title);
        if is_reference {
            ui.weak("Reference pane");
        } else if ui
            .add_enabled(has_image, egui::Button::new("Set as reference"))
            .clicked()
        {
            output.registration_request = Some(RegistrationRequest::SetReference(pane_id));
            ui.close();
        }
        if !is_reference
            && reference_pane.is_some_and(|reference| {
                workspace
                    .panes
                    .iter()
                    .any(|pane| pane.id == reference && pane.image_id.is_some())
            })
        {
            if ui.button("Auto align to reference").clicked() {
                output.registration_request = Some(RegistrationRequest::Automatic(pane_id));
                ui.close();
            }
            if ui.button("Manual alignment…").clicked()
                && let Some(reference) = reference_pane
            {
                state.start_manual_registration(reference, pane_id);
                ui.close();
            }
            if ui.button("Reset alignment").clicked() {
                output.registration_request = Some(RegistrationRequest::Reset(pane_id));
                ui.close();
            }
        }
        ui.separator();
        let can_compare_with_reference = !is_reference
            && has_image
            && reference_pane.is_some_and(|reference| {
                workspace
                    .panes
                    .iter()
                    .any(|pane| pane.id == reference && pane.image_id.is_some())
            });
        ui.add_enabled_ui(can_compare_with_reference, |ui| {
            ui.menu_button("Compare with reference", |ui| {
                if ui.button("Show reference").clicked() {
                    let _ = workspace.set_active(pane_id);
                    state.focused_pane = Some(pane_id);
                    state.comparison_mode = ComparisonMode::Normal;
                    state.blink_latched = true;
                    ui.close();
                }
                if ui.button("Left/right split").clicked() {
                    let _ = workspace.set_active(pane_id);
                    state.focused_pane = Some(pane_id);
                    state.comparison_mode = ComparisonMode::VerticalSplit;
                    state.blink_latched = false;
                    ui.close();
                }
                if ui.button("Top/bottom split").clicked() {
                    let _ = workspace.set_active(pane_id);
                    state.focused_pane = Some(pane_id);
                    state.comparison_mode = ComparisonMode::HorizontalSplit;
                    state.blink_latched = false;
                    ui.close();
                }
            });
        });
        if state.focused_pane == Some(pane_id) {
            if ui.button("Show all panes").clicked() {
                state.focused_pane = None;
                ui.close();
            }
        } else if ui
            .add_enabled(has_image, egui::Button::new("Maximize this pane"))
            .clicked()
        {
            let _ = workspace.set_active(pane_id);
            maximize_pane(state, pane_id);
            ui.close();
        }
        ui.separator();
        if ui.button("Open / replace image…").clicked() {
            output.replace_image_requested = Some(pane_id);
            ui.close();
        }
        if ui
            .add_enabled(has_image, egui::Button::new("Close image"))
            .clicked()
        {
            output.closed_images.push(pane_id);
            ui.close();
        }
        if ui
            .add_enabled(
                workspace.panes.len() > MIN_PANES,
                egui::Button::new("Close pane"),
            )
            .clicked()
        {
            output.removed_panes.push(pane_id);
            ui.close();
        }
        ui.separator();
        if ui
            .add_enabled(has_image, egui::Button::new("Fit image"))
            .clicked()
        {
            if let Some(pane) = workspace.panes.iter_mut().find(|pane| pane.id == pane_id) {
                pane.viewport.set_fit();
            }
            ui.close();
        }
        if ui
            .add_enabled(has_image, egui::Button::new("View at 100% (1:1)"))
            .clicked()
        {
            if let Some(pane) = workspace.panes.iter_mut().find(|pane| pane.id == pane_id) {
                pane.viewport.set_one_to_one();
            }
            ui.close();
        }
        if ui
            .button(if linked {
                "Use free navigation"
            } else {
                "Join synchronized navigation"
            })
            .clicked()
        {
            let _ = workspace.toggle_pane_linked(pane_id);
            ui.close();
        }
        if ui
            .button(if note.is_empty() {
                "Add note…"
            } else {
                "Edit note…"
            })
            .clicked()
        {
            begin_note_edit(state, pane_id, note.clone());
            ui.close();
        }
    });
    let is_focused_comparison_target = focused_comparison_panes(workspace, state).is_some();
    let split_interaction = handle_split_divider(
        ui,
        state,
        pane_id,
        image_rect,
        &response,
        is_active && !is_reference && has_image && is_focused_comparison_target,
    );
    let focus_double_clicked = response.double_clicked()
        && !split_interaction
        && has_image
        && state.manual_registration.is_none();
    if focus_double_clicked {
        let _ = workspace.set_active(pane_id);
        if state.focused_pane == Some(pane_id) {
            state.focused_pane = None;
        } else {
            maximize_pane(state, pane_id);
        }
    }
    let registration_expected = state.expected_registration_pane() == Some(pane_id);
    if response.clicked()
        && registration_expected
        && !split_interaction
        && !focus_double_clicked
        && let Some([image_width, image_height]) = image_size
    {
        let pointer = response.hover_pos().unwrap_or(image_rect.center());
        let pointer_delta = (pointer - image_rect.center()) * pixels_per_point;
        let point = NormalizedPoint {
            x: viewport.center.x
                + f64::from(pointer_delta.x) * viewport.source_pixels_per_physical_pixel
                    / f64::from(image_width.max(1)),
            y: viewport.center.y
                + f64::from(pointer_delta.y) * viewport.source_pixels_per_physical_pixel
                    / f64::from(image_height.max(1)),
        }
        .clamped();
        output.manual_registration_completed = state.record_registration_point(pane_id, point);
    } else if !focus_double_clicked
        && state.manual_registration.is_none()
        && (response.clicked() || response.drag_started())
    {
        let _ = workspace.set_active(pane_id);
    }
    if response.dragged()
        && !split_interaction
        && let Some([image_width, image_height]) = image_size
    {
        let delta = response.drag_motion() * pixels_per_point;
        let normalized_x = f64::from(delta.x) * viewport.source_pixels_per_physical_pixel
            / f64::from(image_width.max(1));
        let normalized_y = f64::from(delta.y) * viewport.source_pixels_per_physical_pixel
            / f64::from(image_height.max(1));
        workspace.pan_pane(pane_id, normalized_x, normalized_y);
    }
    if response.hovered()
        && let Some([image_width, image_height]) = image_size
    {
        let (pinch_factor, scroll_y) =
            ui.input(|input| (input.zoom_delta(), input.smooth_scroll_delta.y));
        let factor = if (pinch_factor - 1.0).abs() > f32::EPSILON {
            pinch_factor
        } else {
            (scroll_y * 0.0025).exp()
        };
        if (factor - 1.0).abs() > 0.0001 {
            let pointer = response.hover_pos().unwrap_or(image_rect.center());
            let pointer_delta = (pointer - image_rect.center()) * pixels_per_point;
            let anchor = NormalizedPoint {
                x: viewport.center.x
                    + f64::from(pointer_delta.x) * viewport.source_pixels_per_physical_pixel
                        / f64::from(image_width.max(1)),
                y: viewport.center.y
                    + f64::from(pointer_delta.y) * viewport.source_pixels_per_physical_pixel
                        / f64::from(image_height.max(1)),
            }
            .clamped();
            workspace.zoom_pane(pane_id, f64::from(factor), anchor);
        }
    }
}

fn paint_focus_icon(painter: &egui::Painter, rect: Rect, restore: bool, color: Color32) {
    let stroke = Stroke::new(1.2, color);
    if restore {
        let back = Rect::from_center_size(rect.center() + egui::vec2(2.0, -2.0), Vec2::splat(9.0));
        let front = Rect::from_center_size(rect.center() + egui::vec2(-2.0, 2.0), Vec2::splat(9.0));
        painter.rect_stroke(back, 0.0, stroke, StrokeKind::Inside);
        painter.rect_stroke(front, 0.0, stroke, StrokeKind::Inside);
        return;
    }

    let icon = Rect::from_center_size(rect.center(), Vec2::splat(12.0));
    let arm = 4.0;
    for (corner, horizontal_direction, vertical_direction) in [
        (icon.left_top(), 1.0, 1.0),
        (icon.right_top(), -1.0, 1.0),
        (icon.left_bottom(), 1.0, -1.0),
        (icon.right_bottom(), -1.0, -1.0),
    ] {
        painter.line_segment(
            [corner, corner + egui::vec2(horizontal_direction * arm, 0.0)],
            stroke,
        );
        painter.line_segment(
            [corner, corner + egui::vec2(0.0, vertical_direction * arm)],
            stroke,
        );
    }
}

fn handle_split_divider(
    ui: &Ui,
    state: &mut UiState,
    pane_id: PaneId,
    image_rect: Rect,
    response: &egui::Response,
    is_comparison_target: bool,
) -> bool {
    let mode = state.comparison_mode;
    if !mode.is_split() || !is_comparison_target || state.manual_registration.is_some() {
        if state.split_dragging == Some(pane_id) {
            state.split_dragging = None;
        }
        return false;
    }

    let divider_position = state.split_position.clamp(0.02, 0.98);
    let hit_rect = match mode {
        ComparisonMode::Normal => return false,
        ComparisonMode::VerticalSplit => {
            let x = egui::lerp(image_rect.left()..=image_rect.right(), divider_position);
            Rect::from_center_size(
                pos2(x, image_rect.center().y),
                Vec2::new(14.0, image_rect.height()),
            )
        }
        ComparisonMode::HorizontalSplit => {
            let y = egui::lerp(image_rect.top()..=image_rect.bottom(), divider_position);
            Rect::from_center_size(
                pos2(image_rect.center().x, y),
                Vec2::new(image_rect.width(), 14.0),
            )
        }
    };
    let (hover_position, press_origin, primary_down) = ui.input(|input| {
        (
            input.pointer.hover_pos(),
            input.pointer.press_origin(),
            input.pointer.primary_down(),
        )
    });
    if hover_position.is_some_and(|position| hit_rect.contains(position)) {
        ui.ctx().set_cursor_icon(match mode {
            ComparisonMode::VerticalSplit => CursorIcon::ResizeHorizontal,
            ComparisonMode::HorizontalSplit => CursorIcon::ResizeVertical,
            ComparisonMode::Normal => CursorIcon::Default,
        });
    }
    if response.drag_started() && press_origin.is_some_and(|position| hit_rect.contains(position)) {
        state.split_dragging = Some(pane_id);
    }
    let divider_double_clicked = response.double_clicked()
        && response
            .hover_pos()
            .is_some_and(|position| hit_rect.contains(position));
    if divider_double_clicked {
        state.split_position = 0.5;
    }

    let dragging = state.split_dragging == Some(pane_id) && primary_down;
    if dragging && let Some(position) = hover_position {
        state.split_position = match mode {
            ComparisonMode::Normal => state.split_position,
            ComparisonMode::VerticalSplit => {
                (position.x - image_rect.left()) / image_rect.width().max(1.0)
            }
            ComparisonMode::HorizontalSplit => {
                (position.y - image_rect.top()) / image_rect.height().max(1.0)
            }
        }
        .clamp(0.02, 0.98);
        ui.ctx().request_repaint();
    }
    if state.split_dragging == Some(pane_id) && !primary_down {
        state.split_dragging = None;
    }
    dragging || divider_double_clicked
}

fn begin_note_edit(state: &mut UiState, pane_id: PaneId, note: String) {
    state.note_editor = Some(NoteEditor {
        pane_id,
        draft: note,
        request_focus: true,
    });
}

fn finish_pane_drag(workspace: &mut Workspace, state: &mut UiState) {
    if let (Some(dragged), Some(target)) = (state.dragged_pane, state.drop_target)
        && dragged != target
        && let (Some(from), Some(to)) = (
            workspace.panes.iter().position(|pane| pane.id == dragged),
            workspace.panes.iter().position(|pane| pane.id == target),
        )
    {
        let _ = workspace.move_pane(from, to);
    }
    state.dragged_pane = None;
    state.drop_target = None;
}

fn draw_note_editor(context: &egui::Context, workspace: &mut Workspace, state: &mut UiState) {
    let Some(editor) = state.note_editor.as_mut() else {
        return;
    };
    let pane_name = workspace
        .panes
        .iter()
        .find(|pane| pane.id == editor.pane_id)
        .map_or_else(|| "Image".to_owned(), |pane| pane.title.clone());
    let mut window_open = true;
    let mut action = None;
    egui::Window::new("Image note")
        .id(egui::Id::new("image-note-editor"))
        .open(&mut window_open)
        .collapsible(false)
        .resizable(false)
        .default_width(420.0)
        .show(context, |ui| {
            ui.label(pane_name);
            ui.weak(format!(
                "Short single-line note · up to {MAX_NOTE_CHARS} characters"
            ));
            let response = ui.add(
                egui::TextEdit::singleline(&mut editor.draft)
                    .char_limit(MAX_NOTE_CHARS)
                    .desired_width(f32::INFINITY),
            );
            if editor.request_focus {
                response.request_focus();
                editor.request_focus = false;
            }
            if ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                action = Some(NoteEditorAction::Save);
            } else if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                action = Some(NoteEditorAction::Cancel);
            }
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    action = Some(NoteEditorAction::Save);
                }
                if ui.button("Clear").clicked() {
                    action = Some(NoteEditorAction::Clear);
                }
                if ui.button("Cancel").clicked() {
                    action = Some(NoteEditorAction::Cancel);
                }
            });
        });
    if !window_open {
        action = Some(NoteEditorAction::Cancel);
    }

    match action {
        Some(NoteEditorAction::Save) => {
            let _ = workspace.set_pane_note(editor.pane_id, &editor.draft);
            state.note_editor = None;
        }
        Some(NoteEditorAction::Clear) => {
            let _ = workspace.set_pane_note(editor.pane_id, "");
            state.note_editor = None;
        }
        Some(NoteEditorAction::Cancel) => state.note_editor = None,
        None => {}
    }
}

#[derive(Clone, Copy)]
enum NoteEditorAction {
    Save,
    Clear,
    Cancel,
}

fn grid_rects(rect: Rect, pane_count: usize, layout_mode: LayoutMode) -> Vec<Rect> {
    if pane_count == 0 {
        return Vec::new();
    }
    let columns = match layout_mode {
        LayoutMode::Auto => (pane_count as f32).sqrt().ceil() as usize,
        LayoutMode::Row => pane_count,
        LayoutMode::Column => 1,
        LayoutMode::TwoColumns => pane_count.min(2),
        LayoutMode::ThreeColumns => pane_count.min(3),
    };
    let rows = pane_count.div_ceil(columns);
    let cell_width =
        (rect.width() - PANE_GAP * (columns.saturating_sub(1) as f32)) / columns as f32;
    let cell_height = (rect.height() - PANE_GAP * (rows.saturating_sub(1) as f32)) / rows as f32;

    (0..pane_count)
        .map(|index| {
            let column = index % columns;
            let row = index / columns;
            let min = pos2(
                rect.left() + column as f32 * (cell_width + PANE_GAP),
                rect.top() + row as f32 * (cell_height + PANE_GAP),
            );
            Rect::from_min_size(min, Vec2::new(cell_width, cell_height))
        })
        .collect()
}

fn update_fit_scale_for_area(workspace: &mut Workspace, pane_id: PaneId, physical_size: [f32; 2]) {
    let Some([image_width, image_height]) = workspace
        .panes
        .iter()
        .find(|pane| pane.id == pane_id)
        .and_then(|pane| pane.image_size)
    else {
        return;
    };
    let fit_scale = (f64::from(image_width) / f64::from(physical_size[0].max(1.0)))
        .max(f64::from(image_height) / f64::from(physical_size[1].max(1.0)));
    workspace.update_pane_fit_scale(pane_id, fit_scale);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_grid_has_no_rectangles() {
        assert!(grid_rects(Rect::EVERYTHING, 0, LayoutMode::Auto).is_empty());
    }

    #[test]
    fn system_theme_falls_back_to_dark_when_unavailable() {
        let context = egui::Context::default();
        context.set_theme(AppTheme::System.egui_theme_preference());
        assert_eq!(context.system_theme(), None);
        assert_eq!(context.theme(), egui::Theme::Dark);
    }

    #[test]
    fn raw_preview_matching_is_enabled_by_default() {
        assert!(UiState::default().match_raw_to_preview);
    }

    #[test]
    fn focused_toolbar_titles_are_bounded_without_splitting_unicode() {
        assert_eq!(compact_label("short.ORF", 12), "short.ORF");
        assert_eq!(compact_label("P7175961-long-name.ORF", 12), "P7175961-...");
        assert_eq!(compact_label("фотография.ORF", 8), "фотог...");
    }

    #[test]
    fn reference_shortcut_requires_a_loaded_active_pane() {
        let mut workspace = Workspace::demo();
        workspace.active_pane = Some(PaneId(3));
        assert_eq!(reference_shortcut_target(&workspace), None);

        workspace.panes[2].image_id = Some(viewer_model::ImageId(7));
        assert_eq!(reference_shortcut_target(&workspace), Some(PaneId(3)));
    }

    #[test]
    fn comparison_requires_loaded_distinct_reference_and_active_panes() {
        let mut workspace = Workspace::demo();
        workspace.panes[0].image_id = Some(viewer_model::ImageId(1));
        workspace.panes[1].image_id = Some(viewer_model::ImageId(2));
        workspace.active_pane = Some(PaneId(2));
        assert_eq!(comparison_panes(&workspace), Some((PaneId(1), PaneId(2))));

        workspace.active_pane = Some(PaneId(1));
        assert_eq!(comparison_panes(&workspace), None);

        workspace.active_pane = Some(PaneId(3));
        assert_eq!(comparison_panes(&workspace), None);
    }

    #[test]
    fn reference_comparison_is_inactive_until_the_target_is_maximized() {
        let mut workspace = Workspace::demo();
        workspace.panes[0].image_id = Some(viewer_model::ImageId(1));
        workspace.panes[1].image_id = Some(viewer_model::ImageId(2));
        workspace.active_pane = Some(PaneId(2));
        let mut state = UiState {
            comparison_mode: ComparisonMode::VerticalSplit,
            blink_latched: true,
            ..UiState::default()
        };

        assert_eq!(comparison_panes(&workspace), Some((PaneId(1), PaneId(2))));
        assert_eq!(focused_comparison_panes(&workspace, &state), None);

        state.focused_pane = Some(PaneId(2));
        assert_eq!(
            focused_comparison_panes(&workspace, &state),
            Some((PaneId(1), PaneId(2)))
        );
    }

    #[test]
    fn ordinary_maximize_starts_with_the_target_image() {
        let mut state = UiState {
            comparison_mode: ComparisonMode::HorizontalSplit,
            blink_latched: true,
            ..UiState::default()
        };

        maximize_pane(&mut state, PaneId(3));

        assert_eq!(state.focused_pane, Some(PaneId(3)));
        assert_eq!(state.comparison_mode, ComparisonMode::Normal);
        assert!(!state.blink_latched);
        assert!(!state.blink_reference_visible());
    }

    #[test]
    fn blink_is_available_only_in_normal_comparison_mode() {
        let mut state = UiState {
            blink_latched: true,
            ..UiState::default()
        };
        assert!(state.blink_reference_visible());

        state.comparison_mode = ComparisonMode::VerticalSplit;
        assert!(!state.blink_reference_visible());
    }

    #[test]
    fn pixel_grid_appears_only_when_pixels_are_large_enough() {
        let mut workspace = Workspace::demo();
        let pane = &mut workspace.panes[0];
        pane.image_size = Some([20, 10]);
        let area = PanePaintArea {
            pane_id: pane.id,
            rect: Rect::from_min_size(pos2(0.0, 0.0), Vec2::new(100.0, 100.0)),
            physical_size: [100.0, 100.0],
        };

        pane.viewport.source_pixels_per_physical_pixel = 0.2;
        assert_eq!(pixel_grid_geometry(pane, &area), None);

        pane.viewport.source_pixels_per_physical_pixel = MIN_PIXEL_GRID_SIZE_PHYSICAL.recip();
        assert!(pixel_grid_geometry(pane, &area).is_some());
    }

    #[test]
    fn pixel_grid_tracks_source_boundaries_through_pan_and_hidpi_scale() {
        let mut workspace = Workspace::demo();
        let pane = &mut workspace.panes[0];
        pane.image_size = Some([20, 20]);
        pane.viewport.center = NormalizedPoint { x: 0.6, y: 0.5 };
        pane.viewport.source_pixels_per_physical_pixel = 0.1;
        let area = PanePaintArea {
            pane_id: pane.id,
            rect: Rect::from_min_size(pos2(0.0, 0.0), Vec2::new(100.0, 100.0)),
            physical_size: [200.0, 200.0],
        };

        let grid = pixel_grid_geometry(pane, &area).expect("1,000% shows a grid");
        assert_eq!(
            grid.image_rect,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(90.0, 100.0))
        );
        assert_eq!(grid.vertical_lines.len(), 19);
        assert_eq!(grid.horizontal_lines.len(), 21);
        assert!((grid.vertical_lines[0] - 0.0).abs() < 0.001);
        assert!((grid.vertical_lines[10] - 50.0).abs() < 0.001);
        assert!((grid.vertical_lines[18] - 90.0).abs() < 0.001);
        assert!((grid.horizontal_lines[0] - 0.0).abs() < 0.001);
        assert!((grid.horizontal_lines[10] - 50.0).abs() < 0.001);
    }

    #[test]
    fn four_panes_form_two_by_two_grid() {
        let panes = grid_rects(
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1000.0, 800.0)),
            4,
            LayoutMode::Auto,
        );
        assert_eq!(panes.len(), 4);
        assert_eq!(panes[0].top(), panes[1].top());
        assert_eq!(panes[0].left(), panes[2].left());
        assert!((panes[1].left() - panes[0].right() - 1.0).abs() < f32::EPSILON);
        assert!((panes[2].top() - panes[0].bottom() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn renderer_rects_are_expressed_in_framebuffer_pixels() {
        let rect = Rect::from_min_size(pos2(10.0, 20.0), Vec2::new(300.0, 200.0));
        assert_eq!(rect_to_physical(rect, 1.5), [15.0, 30.0, 450.0, 300.0]);
    }

    #[test]
    fn row_column_and_fixed_column_layouts_are_distinct() {
        let rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(1200.0, 800.0));
        let row = grid_rects(rect, 4, LayoutMode::Row);
        assert!(row.iter().all(|pane| pane.top() == row[0].top()));

        let column = grid_rects(rect, 4, LayoutMode::Column);
        assert!(column.iter().all(|pane| pane.left() == column[0].left()));

        let three_columns = grid_rects(rect, 6, LayoutMode::ThreeColumns);
        assert_eq!(three_columns[0].top(), three_columns[2].top());
        assert!(three_columns[3].top() > three_columns[0].bottom());
    }

    #[test]
    fn finishing_title_drag_reorders_panes_by_identity() {
        let mut workspace = Workspace::demo();
        let mut state = UiState {
            dragged_pane: Some(PaneId(1)),
            drop_target: Some(PaneId(4)),
            ..UiState::default()
        };

        finish_pane_drag(&mut workspace, &mut state);

        assert_eq!(workspace.panes[3].id, PaneId(1));
        assert_eq!(state.dragged_pane, None);
        assert_eq!(state.drop_target, None);
    }

    #[test]
    fn releasing_drag_without_a_target_keeps_order() {
        let mut workspace = Workspace::demo();
        let original = workspace.panes.clone();
        let mut state = UiState {
            dragged_pane: Some(PaneId(1)),
            drop_target: None,
            ..UiState::default()
        };

        finish_pane_drag(&mut workspace, &mut state);

        assert_eq!(workspace.panes, original);
    }

    #[test]
    fn replacing_the_edited_pane_closes_its_note_editor() {
        let mut state = UiState {
            note_editor: Some(NoteEditor {
                pane_id: PaneId(2),
                draft: "candidate".to_owned(),
                request_focus: false,
            }),
            ..UiState::default()
        };

        state.cancel_note_edit_for(PaneId(1));
        assert!(state.note_editor.is_some());
        state.cancel_note_edit_for(PaneId(2));
        assert!(state.note_editor.is_none());
    }

    #[test]
    fn note_icon_and_title_shortcut_share_editor_setup() {
        let mut state = UiState::default();
        begin_note_edit(&mut state, PaneId(3), "keeper".to_owned());

        let editor = state.note_editor.expect("editor should open");
        assert_eq!(editor.pane_id, PaneId(3));
        assert_eq!(editor.draft, "keeper");
        assert!(editor.request_focus);
    }

    #[test]
    fn manual_registration_collects_alternating_reference_and_target_points() {
        let mut state = UiState::default();
        state.start_manual_registration(PaneId(1), PaneId(2));
        assert_eq!(state.expected_registration_pane(), Some(PaneId(1)));

        let reference_one = NormalizedPoint { x: 0.2, y: 0.3 };
        let target_one = NormalizedPoint { x: 0.4, y: 0.5 };
        let reference_two = NormalizedPoint { x: 0.7, y: 0.2 };
        let target_two = NormalizedPoint { x: 0.8, y: 0.4 };
        assert_eq!(
            state.record_registration_point(PaneId(2), target_one),
            None,
            "a click in the wrong pane must be ignored"
        );
        assert_eq!(
            state.record_registration_point(PaneId(1), reference_one),
            None
        );
        assert_eq!(state.expected_registration_pane(), Some(PaneId(2)));
        assert_eq!(state.record_registration_point(PaneId(2), target_one), None);
        assert_eq!(
            state.record_registration_point(PaneId(1), reference_two),
            None
        );
        let completed = state
            .record_registration_point(PaneId(2), target_two)
            .expect("four alternating points complete registration");

        assert_eq!(completed.reference_pane, PaneId(1));
        assert_eq!(completed.target_pane, PaneId(2));
        assert_eq!(completed.reference_points, [reference_one, reference_two]);
        assert_eq!(completed.target_points, [target_one, target_two]);
        assert!(state.manual_registration.is_none());
    }

    #[test]
    fn replacing_either_manual_registration_pane_cancels_the_session() {
        let mut state = UiState::default();
        state.start_manual_registration(PaneId(1), PaneId(2));
        state.cancel_registration_for(PaneId(3));
        assert!(state.manual_registration.is_some());
        state.cancel_registration_for(PaneId(2));
        assert!(state.manual_registration.is_none());
    }

    #[test]
    fn default_workspace_draws_headlessly_with_one_area_per_pane() {
        let context = egui::Context::default();
        let mut workspace = Workspace::demo();
        workspace.panes[0].image_size = Some([6_000, 4_000]);
        let mut state = UiState::default();
        let mut result = None;

        let _ = context.run_ui(egui::RawInput::default(), |ui| {
            result = Some(draw_workspace(ui, &mut workspace, &mut state));
        });

        let output = result.expect("workspace was drawn");
        assert_eq!(output.paint_areas.len(), workspace.panes.len());
        assert_eq!(output.layout.panes.len(), workspace.panes.len());
        assert!(output.layout.physical_size[0] > 0);
        assert!(output.layout.physical_size[1] > 0);
        assert!(
            workspace.panes[0]
                .viewport
                .fit_source_pixels_per_physical_pixel
                > 1.0
        );
    }

    #[test]
    fn focused_pane_uses_the_full_workspace_and_updates_reference_fit() {
        let context = egui::Context::default();
        let mut workspace = Workspace::demo();
        workspace.panes[0].image_id = Some(viewer_model::ImageId(1));
        workspace.panes[0].image_size = Some([6_000, 4_000]);
        workspace.panes[1].image_id = Some(viewer_model::ImageId(2));
        workspace.panes[1].image_size = Some([3_000, 2_000]);
        workspace.active_pane = Some(PaneId(2));
        let mut state = UiState {
            focused_pane: Some(PaneId(2)),
            ..UiState::default()
        };
        let mut result = None;

        let _ = context.run_ui(egui::RawInput::default(), |ui| {
            result = Some(draw_workspace(ui, &mut workspace, &mut state));
        });

        let output = result.expect("focused workspace was drawn");
        assert_eq!(output.paint_areas.len(), 1);
        assert_eq!(output.paint_areas[0].pane_id, PaneId(2));
        assert_eq!(output.layout.panes.len(), 1);
        let [width, height] = output.paint_areas[0].physical_size;
        let reference_fit = (6_000.0 / f64::from(width)).max(4_000.0 / f64::from(height));
        let target_fit = (3_000.0 / f64::from(width)).max(2_000.0 / f64::from(height));
        assert!(
            (workspace.panes[0]
                .viewport
                .fit_source_pixels_per_physical_pixel
                - reference_fit)
                .abs()
                < 0.000_001
        );
        assert!(
            (workspace.panes[1]
                .viewport
                .fit_source_pixels_per_physical_pixel
                - target_fit)
                .abs()
                < 0.000_001
        );
    }
}
