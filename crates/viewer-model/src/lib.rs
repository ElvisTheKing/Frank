#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const MAX_NOTE_CHARS: usize = 80;
pub const MIN_PANES: usize = 1;
pub const MAX_PANES: usize = 8;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum LayoutMode {
    #[default]
    Auto,
    Row,
    Column,
    TwoColumns,
    ThreeColumns,
}

impl LayoutMode {
    pub const ALL: [Self; 5] = [
        Self::Auto,
        Self::Row,
        Self::Column,
        Self::TwoColumns,
        Self::ThreeColumns,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto grid",
            Self::Row => "Horizontal row",
            Self::Column => "Vertical column",
            Self::TwoColumns => "2 columns",
            Self::ThreeColumns => "3 columns",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ImageMetadata {
    pub megapixels: Option<f64>,
    pub camera: Option<String>,
    pub lens: Option<String>,
    pub bit_depth: Option<usize>,
    pub iso: Option<u32>,
    pub shutter: Option<String>,
    pub aperture: Option<String>,
    pub focal_length: Option<String>,
    pub quality: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TitleFields {
    pub megapixels: bool,
    pub camera: bool,
    pub lens: bool,
    pub bit_depth: bool,
    pub iso: bool,
    pub shutter: bool,
    pub aperture: bool,
    pub focal_length: bool,
    pub quality: bool,
}

impl Default for TitleFields {
    fn default() -> Self {
        Self {
            megapixels: true,
            camera: true,
            lens: false,
            bit_depth: false,
            iso: true,
            shutter: true,
            aperture: true,
            focal_length: true,
            quality: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct PaneId(pub u64);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct ImageId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NormalizedPoint {
    pub x: f64,
    pub y: f64,
}

impl NormalizedPoint {
    #[must_use]
    pub fn clamped(self) -> Self {
        Self {
            x: self.x.clamp(0.0, 1.0),
            y: self.y.clamp(0.0, 1.0),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum SyncMode {
    FitRelative,
    WidthRelative,
    HeightRelative,
    SourcePixels,
}

impl SyncMode {
    pub const ALL: [Self; 4] = [
        Self::FitRelative,
        Self::WidthRelative,
        Self::HeightRelative,
        Self::SourcePixels,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::FitRelative => "Fit-relative",
            Self::WidthRelative => "Width-relative",
            Self::HeightRelative => "Height-relative",
            Self::SourcePixels => "Source pixels",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Viewport {
    pub center: NormalizedPoint,
    /// Source pixels represented by one physical framebuffer pixel.
    /// A value of 1.0 is the pixel-peeping 1:1 view.
    pub source_pixels_per_physical_pixel: f64,
    pub fit_source_pixels_per_physical_pixel: f64,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            center: NormalizedPoint { x: 0.5, y: 0.5 },
            source_pixels_per_physical_pixel: 1.0,
            fit_source_pixels_per_physical_pixel: 1.0,
        }
    }
}

impl Viewport {
    pub const MIN_SCALE: f64 = 1.0 / 64.0;
    pub const MAX_SCALE: f64 = 16_384.0;

    #[must_use]
    pub fn relative_zoom(self) -> f64 {
        self.fit_source_pixels_per_physical_pixel
            / self.source_pixels_per_physical_pixel.max(Self::MIN_SCALE)
    }

    #[must_use]
    pub fn pixel_zoom_percent(self) -> f64 {
        100.0 / self.source_pixels_per_physical_pixel.max(Self::MIN_SCALE)
    }

    pub fn set_one_to_one(&mut self) {
        self.source_pixels_per_physical_pixel = 1.0;
    }

    pub fn set_fit(&mut self) {
        self.source_pixels_per_physical_pixel = self
            .fit_source_pixels_per_physical_pixel
            .max(Self::MIN_SCALE);
        self.center = NormalizedPoint { x: 0.5, y: 0.5 };
    }

    pub fn pan_normalized(&mut self, delta_x: f64, delta_y: f64) {
        self.center.x -= delta_x;
        self.center.y -= delta_y;
        self.center = self.center.clamped();
    }

    pub fn zoom_by(&mut self, factor: f64) {
        if factor.is_finite() && factor > 0.0 {
            self.source_pixels_per_physical_pixel = (self.source_pixels_per_physical_pixel
                / factor)
                .clamp(Self::MIN_SCALE, Self::MAX_SCALE);
        }
    }

    pub fn zoom_by_at(&mut self, factor: f64, anchor: NormalizedPoint) {
        if !factor.is_finite() || factor <= 0.0 {
            return;
        }
        let previous_center = self.center;
        self.zoom_by(factor);
        self.center = NormalizedPoint {
            x: anchor.x + (previous_center.x - anchor.x) / factor,
            y: anchor.y + (previous_center.y - anchor.y) / factor,
        }
        .clamped();
    }

    pub fn update_fit_scale(&mut self, fit_scale: f64) {
        if !fit_scale.is_finite() || fit_scale <= 0.0 {
            return;
        }
        let was_fit = (self.source_pixels_per_physical_pixel
            - self.fit_source_pixels_per_physical_pixel)
            .abs()
            <= f64::EPSILON * 8.0;
        self.fit_source_pixels_per_physical_pixel =
            fit_scale.clamp(Self::MIN_SCALE, Self::MAX_SCALE);
        if was_fit {
            self.source_pixels_per_physical_pixel = self.fit_source_pixels_per_physical_pixel;
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pane {
    pub id: PaneId,
    pub image_id: Option<ImageId>,
    #[serde(default)]
    pub image_size: Option<[u32; 2]>,
    pub title: String,
    pub note: String,
    #[serde(default)]
    pub metadata: ImageMetadata,
    pub linked: bool,
    pub viewport: Viewport,
    /// Comparison-only exposure offset. This is deliberately separate from
    /// RAW baseline exposure, automatic display exposure, and user editing.
    #[serde(default)]
    pub exposure_match_ev: f32,
    #[serde(default)]
    pub preview_match_ev: f32,
    #[serde(default = "default_one")]
    pub preview_match_gamma: f32,
    #[serde(default)]
    pub manual_exposure_ev: f32,
    #[serde(default)]
    pub normalization_confidence: Option<f32>,
    #[serde(default)]
    pub sync_center_offset: NormalizedPoint,
    #[serde(default = "default_one_f64")]
    pub sync_scale_ratio: f64,
}

impl Pane {
    #[must_use]
    pub fn placeholder(id: u64, title: impl Into<String>) -> Self {
        Self {
            id: PaneId(id),
            image_id: None,
            image_size: None,
            title: title.into(),
            note: String::new(),
            metadata: ImageMetadata::default(),
            linked: true,
            viewport: Viewport::default(),
            exposure_match_ev: 0.0,
            preview_match_ev: 0.0,
            preview_match_gamma: 1.0,
            manual_exposure_ev: 0.0,
            normalization_confidence: None,
            sync_center_offset: NormalizedPoint::default(),
            sync_scale_ratio: 1.0,
        }
    }
}

impl Pane {
    #[must_use]
    pub fn formatted_title(&self, fields: TitleFields) -> String {
        let mut parts = vec![self.title.clone()];
        if fields.megapixels
            && let Some(megapixels) = self.metadata.megapixels
        {
            parts.push(format!("{megapixels:.1} MP"));
        }
        if fields.camera
            && let Some(camera) = &self.metadata.camera
        {
            parts.push(camera.clone());
        }
        if fields.lens
            && let Some(lens) = &self.metadata.lens
        {
            parts.push(lens.clone());
        }
        if fields.bit_depth
            && let Some(bit_depth) = self.metadata.bit_depth
        {
            parts.push(format!("{bit_depth} bit"));
        }
        if fields.iso
            && let Some(iso) = self.metadata.iso
        {
            parts.push(format!("ISO {iso}"));
        }
        if fields.shutter
            && let Some(shutter) = &self.metadata.shutter
        {
            parts.push(shutter.clone());
        }
        if fields.aperture
            && let Some(aperture) = &self.metadata.aperture
        {
            parts.push(aperture.clone());
        }
        if fields.focal_length
            && let Some(focal_length) = &self.metadata.focal_length
        {
            parts.push(focal_length.clone());
        }
        if fields.quality
            && let Some(quality) = &self.metadata.quality
        {
            parts.push(quality.clone());
        }
        if self.preview_match_ev.abs() >= 0.005 || (self.preview_match_gamma - 1.0).abs() >= 0.005 {
            parts.push(format!(
                "Preview {:+.2} EV γ{:.2}",
                self.preview_match_ev, self.preview_match_gamma
            ));
        }
        if self.exposure_match_ev.abs() >= 0.005 {
            parts.push(format!(
                "Normalize {:+.2} EV{}",
                self.exposure_match_ev,
                self.normalization_confidence
                    .map(|confidence| format!(" {:.0}%", confidence * 100.0))
                    .unwrap_or_default()
            ));
        }
        if self.manual_exposure_ev.abs() >= 0.005 {
            parts.push(format!("Manual {:+.2} EV", self.manual_exposure_ev));
        }
        parts.join(" · ")
    }
}

impl Pane {
    #[must_use]
    pub fn display_exposure_ev(&self) -> f32 {
        (self.preview_match_ev + self.exposure_match_ev + self.manual_exposure_ev).clamp(-8.0, 8.0)
    }

    #[must_use]
    pub fn display_gamma(&self) -> f32 {
        self.preview_match_gamma.clamp(0.25, 4.0)
    }
}

const fn default_one() -> f32 {
    1.0
}

const fn default_one_f64() -> f64 {
    1.0
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Workspace {
    pub panes: Vec<Pane>,
    pub active_pane: Option<PaneId>,
    pub synchronized: bool,
    pub sync_mode: SyncMode,
    #[serde(default)]
    pub title_fields: TitleFields,
    #[serde(default)]
    pub layout_mode: LayoutMode,
}

impl Workspace {
    #[must_use]
    pub fn demo() -> Self {
        Self {
            panes: (0..4)
                .map(|index| Pane::placeholder(index + 1, format!("Pane {}", index + 1)))
                .collect(),
            active_pane: Some(PaneId(1)),
            synchronized: true,
            sync_mode: SyncMode::FitRelative,
            title_fields: TitleFields::default(),
            layout_mode: LayoutMode::Auto,
        }
    }

    pub fn set_active(&mut self, pane_id: PaneId) -> Result<(), WorkspaceError> {
        if self.panes.iter().any(|pane| pane.id == pane_id) {
            self.active_pane = Some(pane_id);
            Ok(())
        } else {
            Err(WorkspaceError::UnknownPane(pane_id))
        }
    }

    pub fn add_pane(&mut self) -> Result<PaneId, WorkspaceError> {
        if self.panes.len() >= MAX_PANES {
            return Err(WorkspaceError::PaneLimit {
                minimum: MIN_PANES,
                maximum: MAX_PANES,
                current: self.panes.len(),
            });
        }
        let id = self
            .panes
            .iter()
            .map(|pane| pane.id.0)
            .max()
            .unwrap_or_default()
            + 1;
        let pane_id = PaneId(id);
        self.panes.push(Pane::placeholder(id, format!("Pane {id}")));
        self.active_pane = Some(pane_id);
        Ok(pane_id)
    }

    pub fn remove_pane(&mut self, pane_id: PaneId) -> Result<Pane, WorkspaceError> {
        if self.panes.len() <= MIN_PANES {
            return Err(WorkspaceError::PaneLimit {
                minimum: MIN_PANES,
                maximum: MAX_PANES,
                current: self.panes.len(),
            });
        }
        let index = self
            .panes
            .iter()
            .position(|pane| pane.id == pane_id)
            .ok_or(WorkspaceError::UnknownPane(pane_id))?;
        let removed = self.panes.remove(index);
        if self.active_pane == Some(pane_id) {
            self.active_pane = self
                .panes
                .get(index.min(self.panes.len() - 1))
                .map(|pane| pane.id);
        }
        Ok(removed)
    }

    pub fn set_image(
        &mut self,
        pane_id: PaneId,
        image_id: ImageId,
        image_size: [u32; 2],
        title: impl Into<String>,
        metadata: ImageMetadata,
    ) -> Result<(), WorkspaceError> {
        let pane = self
            .panes
            .iter_mut()
            .find(|pane| pane.id == pane_id)
            .ok_or(WorkspaceError::UnknownPane(pane_id))?;
        let is_replacement = pane.image_id != Some(image_id);
        pane.image_id = Some(image_id);
        pane.image_size = Some(image_size);
        pane.title = title.into();
        pane.metadata = metadata;
        if is_replacement {
            pane.viewport.center = NormalizedPoint { x: 0.5, y: 0.5 };
            pane.note.clear();
            pane.preview_match_ev = 0.0;
            pane.preview_match_gamma = 1.0;
            pane.exposure_match_ev = 0.0;
            pane.manual_exposure_ev = 0.0;
            pane.normalization_confidence = None;
            pane.sync_center_offset = NormalizedPoint::default();
            pane.sync_scale_ratio = 1.0;
        }
        Ok(())
    }

    pub fn clear_image(&mut self, pane_id: PaneId, title: impl Into<String>) {
        if let Some(pane) = self.panes.iter_mut().find(|pane| pane.id == pane_id) {
            pane.image_id = None;
            pane.image_size = None;
            pane.title = title.into();
            pane.note.clear();
            pane.metadata = ImageMetadata::default();
            pane.viewport = Viewport::default();
            pane.preview_match_ev = 0.0;
            pane.preview_match_gamma = 1.0;
            pane.exposure_match_ev = 0.0;
            pane.manual_exposure_ev = 0.0;
            pane.normalization_confidence = None;
            pane.sync_center_offset = NormalizedPoint::default();
            pane.sync_scale_ratio = 1.0;
        }
    }

    pub fn update_pane_fit_scale(&mut self, pane_id: PaneId, fit_scale: f64) {
        if let Some(pane) = self.panes.iter_mut().find(|pane| pane.id == pane_id) {
            pane.viewport.update_fit_scale(fit_scale);
        }
    }

    pub fn toggle_pane_linked(&mut self, pane_id: PaneId) -> Result<bool, WorkspaceError> {
        let pane = self
            .panes
            .iter_mut()
            .find(|pane| pane.id == pane_id)
            .ok_or(WorkspaceError::UnknownPane(pane_id))?;
        pane.linked = !pane.linked;
        let linked = pane.linked;
        if linked {
            let reference = self.active_pane.unwrap_or(pane_id);
            self.capture_sync_adjustments(reference);
        }
        Ok(linked)
    }

    pub fn set_synchronized(&mut self, synchronized: bool) {
        if synchronized && !self.synchronized {
            let reference = self.active_pane.or_else(|| {
                self.panes
                    .iter()
                    .find(|pane| pane.linked)
                    .map(|pane| pane.id)
            });
            if let Some(reference) = reference {
                self.capture_sync_adjustments(reference);
            }
        }
        self.synchronized = synchronized;
    }

    pub fn reset_sync_adjustments(&mut self) {
        for pane in &mut self.panes {
            pane.sync_center_offset = NormalizedPoint::default();
            pane.sync_scale_ratio = 1.0;
        }
        if let Some(reference) = self.active_pane {
            self.propagate_synchronized_view(reference);
        }
    }

    fn capture_sync_adjustments(&mut self, reference_id: PaneId) {
        let Some(reference_index) = self.panes.iter().position(|pane| pane.id == reference_id)
        else {
            return;
        };
        let reference_viewport = self.panes[reference_index].viewport;
        let reference_size = self.panes[reference_index].image_size;
        for (index, pane) in self.panes.iter_mut().enumerate() {
            if index == reference_index {
                pane.sync_center_offset = NormalizedPoint::default();
                pane.sync_scale_ratio = 1.0;
                continue;
            }
            let base_scale = synchronized_scale(
                self.sync_mode,
                reference_viewport,
                reference_size,
                pane.viewport,
                pane.image_size,
            );
            pane.sync_center_offset = NormalizedPoint {
                x: pane.viewport.center.x - reference_viewport.center.x,
                y: pane.viewport.center.y - reference_viewport.center.y,
            };
            pane.sync_scale_ratio = (pane.viewport.source_pixels_per_physical_pixel
                / base_scale.max(Viewport::MIN_SCALE))
            .clamp(1.0 / 64.0, 64.0);
        }
    }

    pub fn set_pane_note(
        &mut self,
        pane_id: PaneId,
        note: impl AsRef<str>,
    ) -> Result<(), WorkspaceError> {
        let pane = self
            .panes
            .iter_mut()
            .find(|pane| pane.id == pane_id)
            .ok_or(WorkspaceError::UnknownPane(pane_id))?;
        pane.note = note
            .as_ref()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(MAX_NOTE_CHARS)
            .collect();
        Ok(())
    }

    pub fn move_pane(&mut self, from: usize, to: usize) -> Result<(), WorkspaceError> {
        let len = self.panes.len();
        if from >= len || to >= len {
            return Err(WorkspaceError::InvalidMove { from, to, len });
        }
        if from != to {
            let pane = self.panes.remove(from);
            self.panes.insert(to, pane);
        }
        Ok(())
    }

    pub fn fit_all(&mut self) {
        for pane in &mut self.panes {
            pane.viewport.set_fit();
        }
    }

    pub fn one_to_one_all(&mut self) {
        for pane in &mut self.panes {
            pane.viewport.set_one_to_one();
        }
    }

    pub fn pan_pane(&mut self, pane_id: PaneId, delta_x: f64, delta_y: f64) {
        let source = self
            .panes
            .iter_mut()
            .find(|pane| pane.id == pane_id)
            .map(|pane| {
                pane.viewport.pan_normalized(delta_x, delta_y);
                (pane.viewport, pane.linked)
            });

        if !self.synchronized {
            return;
        }
        let Some((source_viewport, true)) = source else {
            return;
        };

        let source_adjustment = self
            .panes
            .iter()
            .find(|pane| pane.id == pane_id)
            .map(|pane| pane.sync_center_offset)
            .unwrap_or_default();
        let canonical_center = NormalizedPoint {
            x: source_viewport.center.x - source_adjustment.x,
            y: source_viewport.center.y - source_adjustment.y,
        };
        for pane in &mut self.panes {
            if pane.id != pane_id && pane.linked {
                pane.viewport.center = NormalizedPoint {
                    x: canonical_center.x + pane.sync_center_offset.x,
                    y: canonical_center.y + pane.sync_center_offset.y,
                }
                .clamped();
            }
        }
    }

    pub fn zoom_pane(&mut self, pane_id: PaneId, factor: f64, anchor: NormalizedPoint) {
        let Some(source_index) = self.panes.iter().position(|pane| pane.id == pane_id) else {
            return;
        };
        self.panes[source_index].viewport.zoom_by_at(factor, anchor);
        if !self.synchronized || !self.panes[source_index].linked {
            return;
        }

        self.propagate_synchronized_view(pane_id);
    }

    fn propagate_synchronized_view(&mut self, pane_id: PaneId) {
        let Some(source_index) = self.panes.iter().position(|pane| pane.id == pane_id) else {
            return;
        };
        let source_viewport = self.panes[source_index].viewport;
        let source_size = self.panes[source_index].image_size;
        let source_offset = self.panes[source_index].sync_center_offset;
        let source_ratio = self.panes[source_index]
            .sync_scale_ratio
            .max(Viewport::MIN_SCALE);
        let canonical_viewport = Viewport {
            center: NormalizedPoint {
                x: source_viewport.center.x - source_offset.x,
                y: source_viewport.center.y - source_offset.y,
            },
            source_pixels_per_physical_pixel: source_viewport.source_pixels_per_physical_pixel
                / source_ratio,
            fit_source_pixels_per_physical_pixel: source_viewport
                .fit_source_pixels_per_physical_pixel,
        };
        for (index, pane) in self.panes.iter_mut().enumerate() {
            if index == source_index || !pane.linked {
                continue;
            }
            pane.viewport.center = NormalizedPoint {
                x: canonical_viewport.center.x + pane.sync_center_offset.x,
                y: canonical_viewport.center.y + pane.sync_center_offset.y,
            }
            .clamped();
            let synchronized_scale = synchronized_scale(
                self.sync_mode,
                canonical_viewport,
                source_size,
                pane.viewport,
                pane.image_size,
            ) * pane.sync_scale_ratio;
            pane.viewport.source_pixels_per_physical_pixel =
                synchronized_scale.clamp(Viewport::MIN_SCALE, Viewport::MAX_SCALE);
        }
    }
}

fn synchronized_scale(
    mode: SyncMode,
    source_viewport: Viewport,
    source_size: Option<[u32; 2]>,
    target_viewport: Viewport,
    target_size: Option<[u32; 2]>,
) -> f64 {
    match mode {
        SyncMode::FitRelative => {
            target_viewport.fit_source_pixels_per_physical_pixel
                / source_viewport.relative_zoom().max(f64::EPSILON)
        }
        SyncMode::WidthRelative => match (source_size, target_size) {
            (Some([source_width, _]), Some([target_width, _])) => {
                source_viewport.source_pixels_per_physical_pixel * f64::from(target_width)
                    / f64::from(source_width.max(1))
            }
            _ => source_viewport.source_pixels_per_physical_pixel,
        },
        SyncMode::HeightRelative => match (source_size, target_size) {
            (Some([_, source_height]), Some([_, target_height])) => {
                source_viewport.source_pixels_per_physical_pixel * f64::from(target_height)
                    / f64::from(source_height.max(1))
            }
            _ => source_viewport.source_pixels_per_physical_pixel,
        },
        SyncMode::SourcePixels => source_viewport.source_pixels_per_physical_pixel,
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum WorkspaceError {
    #[error("unknown pane {0:?}")]
    UnknownPane(PaneId),
    #[error("cannot move pane from {from} to {to}; workspace has {len} panes")]
    InvalidMove { from: usize, to: usize, len: usize },
    #[error("pane count {current} is outside the supported {minimum}..={maximum} range")]
    PaneLimit {
        minimum: usize,
        maximum: usize,
        current: usize,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_to_one_is_one_source_pixel_per_physical_pixel() {
        let mut viewport = Viewport {
            source_pixels_per_physical_pixel: 8.0,
            ..Viewport::default()
        };
        viewport.set_one_to_one();
        assert_eq!(viewport.source_pixels_per_physical_pixel, 1.0);
        assert_eq!(viewport.pixel_zoom_percent(), 100.0);
    }

    #[test]
    fn free_views_become_persistent_sync_registration() {
        let mut workspace = Workspace::demo();
        workspace.sync_mode = SyncMode::SourcePixels;
        workspace.set_synchronized(false);
        workspace.panes[0].viewport.center = NormalizedPoint { x: 0.4, y: 0.45 };
        workspace.panes[0].viewport.source_pixels_per_physical_pixel = 2.0;
        workspace.panes[1].viewport.center = NormalizedPoint { x: 0.6, y: 0.55 };
        workspace.panes[1].viewport.source_pixels_per_physical_pixel = 3.0;

        workspace.set_synchronized(true);
        workspace.zoom_pane(PaneId(1), 2.0, NormalizedPoint { x: 0.4, y: 0.45 });
        assert!(
            (workspace.panes[1].viewport.center.x - workspace.panes[0].viewport.center.x - 0.2)
                .abs()
                < 1.0e-9
        );
        assert!(
            (workspace.panes[1].viewport.source_pixels_per_physical_pixel - 1.5).abs() < 1.0e-9
        );

        workspace.pan_pane(PaneId(1), 0.05, 0.02);
        assert!(
            (workspace.panes[1].viewport.center.x - workspace.panes[0].viewport.center.x - 0.2)
                .abs()
                < 1.0e-9
        );
        assert!(
            (workspace.panes[1].viewport.center.y - workspace.panes[0].viewport.center.y - 0.1)
                .abs()
                < 1.0e-9
        );
    }

    #[test]
    fn reset_sync_registration_realigns_linked_panes() {
        let mut workspace = Workspace::demo();
        workspace.sync_mode = SyncMode::SourcePixels;
        workspace.set_synchronized(false);
        workspace.panes[1].viewport.center = NormalizedPoint { x: 0.7, y: 0.3 };
        workspace.panes[1].viewport.source_pixels_per_physical_pixel = 2.5;
        workspace.set_synchronized(true);

        workspace.reset_sync_adjustments();
        assert_eq!(
            workspace.panes[0].viewport.center,
            workspace.panes[1].viewport.center
        );
        assert_eq!(
            workspace.panes[0].viewport.source_pixels_per_physical_pixel,
            workspace.panes[1].viewport.source_pixels_per_physical_pixel
        );
    }

    #[test]
    fn pointer_anchored_zoom_keeps_the_anchor_stationary() {
        let mut viewport = Viewport::default();
        viewport.zoom_by_at(2.0, NormalizedPoint { x: 0.75, y: 0.25 });
        assert_eq!(viewport.center, NormalizedPoint { x: 0.625, y: 0.375 });
        assert_eq!(viewport.source_pixels_per_physical_pixel, 0.5);
    }

    #[test]
    fn viewport_rejects_invalid_zoom_and_clamps_navigation() {
        let mut viewport = Viewport::default();
        viewport.zoom_by(0.0);
        viewport.zoom_by(f64::NAN);
        assert_eq!(viewport.source_pixels_per_physical_pixel, 1.0);

        viewport.zoom_by(f64::MAX);
        assert_eq!(
            viewport.source_pixels_per_physical_pixel,
            Viewport::MIN_SCALE
        );
        viewport.zoom_by(f64::MIN_POSITIVE);
        assert_eq!(
            viewport.source_pixels_per_physical_pixel,
            Viewport::MAX_SCALE
        );

        viewport.pan_normalized(-2.0, 2.0);
        assert_eq!(viewport.center, NormalizedPoint { x: 1.0, y: 0.0 });
    }

    #[test]
    fn fit_scale_updates_only_views_that_are_still_fitted() {
        let mut fitted = Viewport::default();
        fitted.update_fit_scale(4.0);
        assert_eq!(fitted.fit_source_pixels_per_physical_pixel, 4.0);
        assert_eq!(fitted.source_pixels_per_physical_pixel, 4.0);

        fitted.set_one_to_one();
        fitted.update_fit_scale(8.0);
        assert_eq!(fitted.fit_source_pixels_per_physical_pixel, 8.0);
        assert_eq!(fitted.source_pixels_per_physical_pixel, 1.0);

        fitted.update_fit_scale(f64::NAN);
        fitted.update_fit_scale(0.0);
        assert_eq!(fitted.fit_source_pixels_per_physical_pixel, 8.0);
    }

    #[test]
    fn synchronized_pan_updates_only_linked_peers() {
        let mut workspace = Workspace::demo();
        workspace.panes[2].linked = false;
        workspace.pan_pane(PaneId(1), 0.1, -0.2);

        assert_eq!(
            workspace.panes[0].viewport.center,
            workspace.panes[1].viewport.center
        );
        assert_ne!(
            workspace.panes[0].viewport.center,
            workspace.panes[2].viewport.center
        );
        assert_eq!(
            workspace.panes[0].viewport.center,
            workspace.panes[3].viewport.center
        );
    }

    #[test]
    fn moving_a_pane_preserves_identity() {
        let mut workspace = Workspace::demo();
        workspace.move_pane(0, 3).expect("valid move");
        assert_eq!(workspace.panes[3].id, PaneId(1));
    }

    #[test]
    fn invalid_move_is_reported() {
        let mut workspace = Workspace::demo();
        assert_eq!(
            workspace.move_pane(0, 9),
            Err(WorkspaceError::InvalidMove {
                from: 0,
                to: 9,
                len: 4,
            })
        );
    }

    #[test]
    fn pane_can_leave_and_rejoin_the_sync_group() {
        let mut workspace = Workspace::demo();
        assert!(!workspace.toggle_pane_linked(PaneId(2)).expect("known pane"));
        assert!(workspace.toggle_pane_linked(PaneId(2)).expect("known pane"));
    }

    #[test]
    fn notes_are_single_line_trimmed_and_length_bounded() {
        let mut workspace = Workspace::demo();
        let long_note = format!("  sharpest\nframe   {}", "x".repeat(100));
        workspace
            .set_pane_note(PaneId(1), long_note)
            .expect("known pane");

        assert!(!workspace.panes[0].note.contains('\n'));
        assert!(!workspace.panes[0].note.contains("  "));
        assert_eq!(workspace.panes[0].note.chars().count(), MAX_NOTE_CHARS);
    }

    #[test]
    fn replacing_an_image_clears_its_old_note() {
        let mut workspace = Workspace::demo();
        workspace
            .set_pane_note(PaneId(1), "old image")
            .expect("known pane");
        workspace.clear_image(PaneId(1), "replacement");

        assert!(workspace.panes[0].note.is_empty());
    }

    #[test]
    fn title_fields_format_structured_metadata_live() {
        let mut pane = Pane::placeholder(1, "P123.ORF");
        pane.metadata = ImageMetadata {
            megapixels: Some(80.6),
            camera: Some("OM-5".to_owned()),
            lens: Some("12-45mm".to_owned()),
            iso: Some(200),
            shutter: Some("1/500 s".to_owned()),
            aperture: Some("f/4.0".to_owned()),
            focal_length: Some("25 mm".to_owned()),
            quality: Some("preview".to_owned()),
            ..ImageMetadata::default()
        };

        let default_title = pane.formatted_title(TitleFields::default());
        assert!(default_title.contains("P123.ORF · 80.6 MP · OM-5"));
        assert!(default_title.contains("ISO 200 · 1/500 s · f/4.0 · 25 mm"));
        assert!(!default_title.contains("12-45mm"));

        let lens_only = pane.formatted_title(TitleFields {
            lens: true,
            megapixels: false,
            camera: false,
            iso: false,
            shutter: false,
            aperture: false,
            focal_length: false,
            quality: false,
            ..TitleFields::default()
        });
        assert_eq!(lens_only, "P123.ORF · 12-45mm");
    }

    #[test]
    fn title_and_display_adjustments_are_bounded_and_visible() {
        let mut pane = Pane::placeholder(1, "candidate");
        pane.preview_match_ev = 2.0;
        pane.preview_match_gamma = 10.0;
        pane.exposure_match_ev = 5.0;
        pane.manual_exposure_ev = 4.0;
        pane.normalization_confidence = Some(0.875);

        let title = pane.formatted_title(TitleFields::default());
        assert!(title.contains("Preview +2.00 EV γ10.00"));
        assert!(title.contains("Normalize +5.00 EV 88%"));
        assert!(title.contains("Manual +4.00 EV"));
        assert_eq!(pane.display_exposure_ev(), 8.0);
        assert_eq!(pane.display_gamma(), 4.0);
    }

    #[test]
    fn comparison_exposure_starts_neutral() {
        let pane = Pane::placeholder(1, "reference");
        assert_eq!(pane.exposure_match_ev, 0.0);
        assert_eq!(pane.manual_exposure_ev, 0.0);
        assert_eq!(pane.display_gamma(), 1.0);
    }

    #[test]
    fn panes_can_be_added_and_removed_with_stable_unique_ids() {
        let mut workspace = Workspace::demo();
        let added = workspace.add_pane().expect("below maximum");
        assert_eq!(added, PaneId(5));
        assert_eq!(workspace.active_pane, Some(PaneId(5)));

        let removed = workspace.remove_pane(added).expect("above minimum");
        assert_eq!(removed.id, PaneId(5));
        assert_eq!(workspace.active_pane, Some(PaneId(4)));
    }

    #[test]
    fn pane_count_is_bounded_between_one_and_eight() {
        let mut workspace = Workspace::demo();
        while workspace.panes.len() < MAX_PANES {
            workspace.add_pane().expect("below maximum");
        }
        assert!(matches!(
            workspace.add_pane(),
            Err(WorkspaceError::PaneLimit { current: 8, .. })
        ));
        while workspace.panes.len() > MIN_PANES {
            let pane_id = workspace.panes.last().expect("pane exists").id;
            workspace.remove_pane(pane_id).expect("above minimum");
        }
        assert!(matches!(
            workspace.remove_pane(workspace.panes[0].id),
            Err(WorkspaceError::PaneLimit { current: 1, .. })
        ));
    }

    #[test]
    fn refreshing_the_same_image_preserves_navigation() {
        let mut workspace = Workspace::demo();
        workspace
            .set_image(
                PaneId(1),
                ImageId(9),
                [4_000, 3_000],
                "preview",
                ImageMetadata::default(),
            )
            .expect("pane exists");
        workspace.panes[0].viewport.center = NormalizedPoint { x: 0.3, y: 0.7 };
        workspace.panes[0].viewport.source_pixels_per_physical_pixel = 0.75;

        workspace
            .set_image(
                PaneId(1),
                ImageId(9),
                [8_000, 6_000],
                "full raw",
                ImageMetadata::default(),
            )
            .expect("pane exists");

        assert_eq!(
            workspace.panes[0].viewport.center,
            NormalizedPoint { x: 0.3, y: 0.7 }
        );
        assert_eq!(
            workspace.panes[0].viewport.source_pixels_per_physical_pixel,
            0.75
        );
    }

    #[test]
    fn replacing_an_image_resets_image_specific_state() {
        let mut workspace = Workspace::demo();
        workspace.panes[0].note = "old image".to_owned();
        workspace.panes[0].preview_match_ev = 1.0;
        workspace.panes[0].preview_match_gamma = 0.6;
        workspace.panes[0].exposure_match_ev = -1.0;
        workspace.panes[0].manual_exposure_ev = 0.5;
        workspace.panes[0].normalization_confidence = Some(0.9);

        workspace
            .set_image(
                PaneId(1),
                ImageId(10),
                [6_000, 4_000],
                "new image",
                ImageMetadata::default(),
            )
            .expect("pane exists");

        let pane = &workspace.panes[0];
        assert!(pane.note.is_empty());
        assert_eq!(pane.preview_match_ev, 0.0);
        assert_eq!(pane.preview_match_gamma, 1.0);
        assert_eq!(pane.exposure_match_ev, 0.0);
        assert_eq!(pane.manual_exposure_ev, 0.0);
        assert_eq!(pane.normalization_confidence, None);
    }

    #[test]
    fn sync_scale_modes_map_different_image_dimensions() {
        let source = Viewport {
            source_pixels_per_physical_pixel: 2.0,
            fit_source_pixels_per_physical_pixel: 4.0,
            ..Viewport::default()
        };
        let target = Viewport {
            fit_source_pixels_per_physical_pixel: 3.0,
            ..Viewport::default()
        };

        assert_eq!(
            synchronized_scale(
                SyncMode::FitRelative,
                source,
                Some([4_000, 2_000]),
                target,
                Some([2_000, 4_000]),
            ),
            1.5
        );
        assert_eq!(
            synchronized_scale(
                SyncMode::WidthRelative,
                source,
                Some([4_000, 2_000]),
                target,
                Some([2_000, 4_000]),
            ),
            1.0
        );
        assert_eq!(
            synchronized_scale(
                SyncMode::HeightRelative,
                source,
                Some([4_000, 2_000]),
                target,
                Some([2_000, 4_000]),
            ),
            4.0
        );
        assert_eq!(
            synchronized_scale(SyncMode::SourcePixels, source, None, target, None),
            2.0
        );
        assert_eq!(
            synchronized_scale(SyncMode::WidthRelative, source, None, target, None),
            2.0
        );
    }

    #[test]
    fn unknown_pane_operations_report_errors_without_mutation() {
        let mut workspace = Workspace::demo();
        let original = workspace.clone();

        assert_eq!(
            workspace.set_active(PaneId(99)),
            Err(WorkspaceError::UnknownPane(PaneId(99)))
        );
        assert_eq!(
            workspace.toggle_pane_linked(PaneId(99)),
            Err(WorkspaceError::UnknownPane(PaneId(99)))
        );
        assert_eq!(
            workspace.set_pane_note(PaneId(99), "missing"),
            Err(WorkspaceError::UnknownPane(PaneId(99)))
        );
        assert_eq!(workspace, original);
    }
}
