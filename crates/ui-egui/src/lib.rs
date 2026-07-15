#![forbid(unsafe_code)]

use egui::{Align2, Color32, CursorIcon, FontId, Rect, Sense, Stroke, StrokeKind, Ui, Vec2, pos2};
use renderer_wgpu::{PaneLayout, WorkspaceLayout};
use viewer_model::{
    LayoutMode, MAX_NOTE_CHARS, MAX_PANES, MIN_PANES, NormalizedPoint, PaneId, SyncMode,
    TitleFields, Workspace,
};

const TOOLBAR_HEIGHT: f32 = 44.0;
const TOOLBAR_MARGIN: f32 = 8.0;
const TOOLBAR_GROUP_GAP: f32 = 10.0;
const TITLE_HEIGHT: f32 = 46.0;
const LINK_CONTROL_WIDTH: f32 = 46.0;
const NOTE_CONTROL_WIDTH: f32 = 28.0;
const CLOSE_CONTROL_WIDTH: f32 = 28.0;
const PANE_GAP: f32 = 1.0;

#[derive(Debug)]
pub struct UiState {
    pub show_pixel_grid: bool,
    pub show_pane_controls: bool,
    pub develop_raws_on_load: bool,
    pub raw_mode: RawModeChoice,
    pub sync_adjustments: bool,
    pub theme: AppTheme,
    dragged_pane: Option<PaneId>,
    drop_target: Option<PaneId>,
    note_editor: Option<NoteEditor>,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            show_pixel_grid: false,
            show_pane_controls: true,
            develop_raws_on_load: false,
            raw_mode: RawModeChoice::default(),
            sync_adjustments: false,
            theme: AppTheme::default(),
            dragged_pane: None,
            drop_target: None,
            note_editor: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AppTheme {
    #[default]
    Dark,
    Light,
}

impl AppTheme {
    const fn egui_theme(self) -> egui::Theme {
        match self {
            Self::Dark => egui::Theme::Dark,
            Self::Light => egui::Theme::Light,
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

fn palette(theme: AppTheme) -> UiPalette {
    match theme {
        AppTheme::Dark => UiPalette {
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
        AppTheme::Light => UiPalette {
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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RawModeChoice {
    AsShot,
    #[default]
    AutoReference,
    LinearDiagnostic,
}

impl RawModeChoice {
    const ALL: [Self; 3] = [Self::AsShot, Self::AutoReference, Self::LinearDiagnostic];

    const fn label(self) -> &'static str {
        match self {
            Self::AsShot => "As shot",
            Self::AutoReference => "Auto reference",
            Self::LinearDiagnostic => "Linear diagnostic",
        }
    }
}

impl UiState {
    pub fn cancel_note_edit_for(&mut self, pane_id: PaneId) {
        if self
            .note_editor
            .as_ref()
            .is_some_and(|editor| editor.pane_id == pane_id)
        {
            self.note_editor = None;
        }
    }
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
    pub raw_develop_requested: Option<RawModeChoice>,
    pub exposure_match_requested: bool,
    pub preview_match_requested: bool,
    pub exposure_match_reset_requested: bool,
}

#[derive(Default)]
struct ToolbarOutput {
    open_requested: bool,
    replace_image_requested: Option<PaneId>,
    closed_images: Vec<PaneId>,
    added_panes: Vec<PaneId>,
    removed_panes: Vec<PaneId>,
    view_one_to_one_requested: bool,
    raw_develop_requested: Option<RawModeChoice>,
    exposure_match_requested: bool,
    preview_match_requested: bool,
    exposure_match_reset_requested: bool,
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
    ui.ctx().set_theme(state.theme.egui_theme());
    let colors = palette(state.theme);
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

    let pane_rects = grid_rects(workspace_rect, workspace.panes.len(), workspace.layout_mode);
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

    for (index, pane_rect) in pane_rects.into_iter().enumerate() {
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
        if let Some([image_width, image_height]) = workspace.panes[index].image_size {
            let fit_scale = (f64::from(image_width) / f64::from(physical_size[0].max(1.0)))
                .max(f64::from(image_height) / f64::from(physical_size[1].max(1.0)));
            workspace.update_pane_fit_scale(pane_id, fit_scale);
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

    if ui.input(|input| input.pointer.any_released()) {
        finish_pane_drag(workspace, state);
    }
    toolbar_output.removed_panes.dedup();
    let requested_removals = std::mem::take(&mut toolbar_output.removed_panes);
    for pane_id in requested_removals {
        if workspace.remove_pane(pane_id).is_ok() {
            state.cancel_note_edit_for(pane_id);
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
        exposure_match_requested: toolbar_output.exposure_match_requested,
        preview_match_requested: toolbar_output.preview_match_requested,
        exposure_match_reset_requested: toolbar_output.exposure_match_reset_requested,
    }
}

fn rect_to_physical(rect: Rect, pixels_per_point: f32) -> [f32; 4] {
    [
        rect.left() * pixels_per_point,
        rect.top() * pixels_per_point,
        rect.width() * pixels_per_point,
        rect.height() * pixels_per_point,
    ]
}

fn draw_toolbar(ui: &mut Ui, workspace: &mut Workspace, state: &mut UiState) -> ToolbarOutput {
    let mut output = ToolbarOutput::default();
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
            output.added_panes.push(pane_id);
        }
        ui.menu_button("Layout", |ui| {
            for mode in LayoutMode::ALL {
                ui.selectable_value(&mut workspace.layout_mode, mode, mode.label());
            }
        });
        ui.add_space(TOOLBAR_GROUP_GAP);
        if ui.button("Fit").clicked() {
            workspace.fit_all();
        }
        if ui
            .button("100%")
            .on_hover_text("View at 100% and develop the active RAW at full source resolution")
            .clicked()
        {
            workspace.one_to_one_all();
            output.view_one_to_one_requested = true;
        }
        ui.add_space(TOOLBAR_GROUP_GAP);
        let mut synchronized = workspace.synchronized;
        if ui.checkbox(&mut synchronized, "Sync").changed() {
            workspace.set_synchronized(synchronized);
        }
        ui.menu_button(workspace.sync_mode.label(), |ui| {
            ui.strong("Synchronization");
            for mode in SyncMode::ALL {
                ui.selectable_value(&mut workspace.sync_mode, mode, mode.label());
            }
            ui.separator();
            if ui.button("Reset alignment").clicked() {
                workspace.reset_sync_adjustments();
                ui.close();
            }
        });
        ui.add_space(TOOLBAR_GROUP_GAP);
        if ui
            .selectable_label(state.show_pane_controls, "Controls")
            .on_hover_text("Show or hide pane headers for borderless comparison")
            .clicked()
        {
            state.show_pane_controls = !state.show_pane_controls;
        }
        ui.add_space(TOOLBAR_GROUP_GAP);
        ui.menu_button("RAW", |ui| {
            ui.strong("Development recipe");
            for mode in RawModeChoice::ALL {
                ui.selectable_value(&mut state.raw_mode, mode, mode.label());
            }
            ui.checkbox(&mut state.develop_raws_on_load, "Develop RAWs on load");
            ui.separator();
            if ui.button("Develop active RAW").clicked() {
                output.raw_develop_requested = Some(state.raw_mode);
                ui.close();
            }
            if ui.button("Match embedded preview").clicked() {
                output.preview_match_requested = true;
                ui.close();
            }
        });
        ui.menu_button("Adjust", |ui| {
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
                if ui.button("Normalize panes to active").clicked() {
                    output.exposure_match_requested = true;
                    ui.close();
                }
                if ui.button("Reset matching").clicked() {
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
        ui.menu_button("View", |ui| {
            ui.strong("Theme");
            ui.horizontal(|ui| {
                ui.selectable_value(&mut state.theme, AppTheme::Dark, "Dark");
                ui.selectable_value(&mut state.theme, AppTheme::Light, "Light");
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
            ui.checkbox(&mut state.show_pixel_grid, "Pixel grid");
            ui.checkbox(&mut state.show_pane_controls, "Pane controls");
        });
    });
    output
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
    let link_rect = Rect::from_min_max(
        pos2(close_rect.left() - LINK_CONTROL_WIDTH, title_rect.top()),
        pos2(close_rect.left() - 2.0, title_rect.bottom()),
    )
    .shrink2(Vec2::new(2.0, 7.0));
    let note_rect = Rect::from_min_max(
        pos2(link_rect.left() - NOTE_CONTROL_WIDTH, title_rect.top()),
        pos2(link_rect.left() - 2.0, title_rect.bottom()),
    )
    .shrink2(Vec2::new(3.0, 7.0));
    let drag_rect = Rect::from_min_max(
        title_rect.min,
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
    let pane = &workspace.panes[pane_index];
    let image_size = pane.image_size;
    let viewport = pane.viewport;
    let has_image = pane.image_id.is_some();
    let linked = pane.linked;
    let note = pane.note.clone();
    let pane_title = pane.title.clone();
    let zoom_percent = pane.viewport.pixel_zoom_percent();
    let formatted_title = pane.formatted_title(title_fields);
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
        ui.painter()
            .with_clip_rect(name_rect.shrink2(Vec2::new(8.0, 0.0)))
            .text(
                pos2(title_rect.left() + 8.0, title_rect.top() + 13.0),
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
                    pos2(title_rect.left() + 8.0, title_rect.bottom() - 11.0),
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
    if response.clicked() || response.drag_started() {
        let _ = workspace.set_active(pane_id);
    }
    if response.dragged()
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
