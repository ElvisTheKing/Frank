use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use image_loader::{
    DecodeQuality, DecodeReservation, LoadHandle, LuminanceGrid, RawDevelopOptions, RawDiagnostics,
    RawRecipe,
};
use viewer_model::ImageId;

#[derive(Default)]
pub(crate) struct PaneRuntime {
    pub(crate) image_id: Option<ImageId>,
    pub(crate) handle: Option<LoadHandle>,
    pub(crate) status: PaneStatus,
    pub(crate) source_path: Option<PathBuf>,
    pub(crate) is_raw_source: bool,
    pub(crate) full_raw_pending: bool,
    pub(crate) pending_raw_options: Option<RawDevelopOptions>,
    pub(crate) full_raw_error: Option<String>,
    pub(crate) raw_recipe: Option<RawRecipe>,
    pub(crate) raw_diagnostics: Option<RawDiagnostics>,
    pub(crate) display_linear_stats: Option<[f32; 5]>,
    pub(crate) preview_linear_stats: Option<[f32; 5]>,
    pub(crate) luminance_grid: Option<LuminanceGrid>,
    pub(crate) display_size: Option<[u32; 2]>,
    pub(crate) source_size: Option<[u32; 2]>,
    pub(crate) quality: Option<DecodeQuality>,
}

#[derive(Default)]
pub(crate) enum PaneStatus {
    #[default]
    Empty,
    Decoding {
        path: PathBuf,
    },
    Uploading {
        decode_time: Duration,
        total_bytes: usize,
        source_size: [u32; 2],
        quality: DecodeQuality,
        bit_depth: Option<usize>,
        reservation: Option<DecodeReservation>,
    },
    Ready {
        decode_time: Duration,
        total_bytes: usize,
        source_size: [u32; 2],
        quality: DecodeQuality,
        bit_depth: Option<usize>,
    },
    Error {
        message: String,
    },
}

pub(crate) fn file_display_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .map_or_else(|| path.display().to_string(), ToOwned::to_owned)
}
