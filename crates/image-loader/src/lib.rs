#![forbid(unsafe_code)]

use std::{
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crossbeam_channel::{Receiver, Sender, TryRecvError, unbounded};
use image::{DynamicImage, ImageDecoder, RgbaImage, codecs::jpeg::JpegDecoder};
use moxcms::{ColorProfile, Layout, TransformOptions};
use rawler::{
    Orientation,
    decoders::RawDecodeParams,
    get_decoder,
    imgop::develop::{Intermediate, ProcessingStep, RawDevelop},
    rawsource::RawSource,
};
use thiserror::Error;

mod raw_preview;

pub use raw_preview::{
    EmbeddedPreviewInfo, decode_largest_embedded_jpeg, decode_largest_embedded_jpeg_from_reader,
    embedded_preview_info, embedded_preview_info_from_reader,
};

pub const DECODE_TILE_SIZE: u32 = 512;
pub const DEFAULT_DECODE_BUDGET_BYTES: usize = 160 * 1024 * 1024;
const RAW_PREVIEW_ESTIMATED_BYTES: usize = 80 * 1024 * 1024;
const MAX_PARALLEL_FULL_RAW_DECODE: usize = 2;

#[must_use]
pub fn supported_extensions() -> Vec<&'static str> {
    let mut extensions = vec!["jpg", "jpeg"];
    for &extension in rawler::decoders::supported_extensions() {
        if !extensions.contains(&extension) {
            extensions.push(extension);
        }
    }
    extensions
}

#[must_use]
pub fn is_raw_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            rawler::decoders::supported_extensions()
                .iter()
                .any(|supported| extension.eq_ignore_ascii_case(supported))
        })
}

#[derive(Debug)]
pub struct DecodedImage {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub source_width: u32,
    pub source_height: u32,
    pub quality: DecodeQuality,
    pub camera: Option<String>,
    pub lens: Option<String>,
    pub bit_depth: Option<usize>,
    pub capture: CaptureMetadata,
    pub raw_recipe: Option<RawRecipe>,
    pub raw_diagnostics: Option<RawDiagnostics>,
    pub display_linear_luminance_percentiles: [f32; 5],
    pub display_linear_rgb_medians: [f32; 3],
    pub luminance_grid: LuminanceGrid,
    pub color_grid: ColorGrid,
    pub registration_image: RegistrationImage,
    pub tiles: Vec<DecodedTile>,
    pub decode_time: Duration,
    reservation: Option<DecodeReservation>,
}

pub const LUMINANCE_GRID_SIZE: usize = 256;
pub const REGISTRATION_IMAGE_LONG_EDGE: usize = 640;

#[derive(Clone, Debug, PartialEq)]
pub struct LuminanceGrid {
    pub width: usize,
    pub height: usize,
    pub values: Vec<f32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ColorGrid {
    pub width: usize,
    pub height: usize,
    pub values: Vec<[f32; 3]>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RegistrationImage {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
}

pub const RAW_RECIPE_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq)]
pub struct RawRecipe {
    pub version: u32,
    pub developer: &'static str,
    pub white_balance_source: WhiteBalanceSource,
    pub baseline_exposure_ev: f32,
    pub automatic_exposure_ev: f32,
    pub comparison_match_ev: f32,
    pub tone_map: ToneMap,
    pub highlight_method: HighlightMethod,
    pub display_mode: RawDisplayMode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RawDisplayMode {
    AsShot,
    Reference,
    PreviewMatched,
    LinearDiagnostic,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RawDevelopOptions {
    pub mode: RawDisplayMode,
    pub comparison_match_ev: f32,
}

impl Default for RawDevelopOptions {
    fn default() -> Self {
        Self {
            mode: RawDisplayMode::AsShot,
            comparison_match_ev: 0.0,
        }
    }
}

impl Default for RawRecipe {
    fn default() -> Self {
        Self {
            version: RAW_RECIPE_VERSION,
            developer: "rawler-0.7/default",
            white_balance_source: WhiteBalanceSource::Camera,
            baseline_exposure_ev: 0.0,
            automatic_exposure_ev: 0.0,
            comparison_match_ev: 0.0,
            tone_map: ToneMap::SrgbTransferOnly,
            highlight_method: HighlightMethod::RawlerDefault,
            display_mode: RawDisplayMode::AsShot,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WhiteBalanceSource {
    Camera,
    Fallback,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToneMap {
    SrgbTransferOnly,
    ReferenceSigmoid,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HighlightMethod {
    RawlerDefault,
    SoftShoulder,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RawDiagnostics {
    pub black_levels: Vec<f32>,
    pub white_levels: Vec<f32>,
    pub white_balance: [f32; 4],
    pub display_channel_maxima: [u8; 3],
    pub display_clipped_pixels: u64,
    pub display_pixel_count: u64,
    pub display_luminance_percentiles: [f32; 5],
    pub linear_luminance_percentiles: Option<[f32; 5]>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CaptureMetadata {
    pub iso: Option<u32>,
    pub shutter: Option<String>,
    pub aperture: Option<String>,
    pub focal_length: Option<String>,
    pub exposure_time_seconds: Option<f64>,
    pub fnumber: Option<f64>,
}

impl CaptureMetadata {
    #[must_use]
    pub fn exposure_value_ev100(&self) -> Option<f64> {
        let aperture = self.fnumber?;
        let seconds = self.exposure_time_seconds?;
        let iso = f64::from(self.iso?);
        let ev = (aperture * aperture / seconds).log2() - (iso / 100.0).log2();
        ev.is_finite().then_some(ev)
    }
}

impl DecodedImage {
    #[must_use]
    pub fn byte_len(&self) -> usize {
        self.tiles.iter().map(|tile| tile.rgba.len()).sum()
    }

    pub fn take_reservation(&mut self) -> Option<DecodeReservation> {
        self.reservation.take()
    }
}

pub struct DecodeReservation {
    permit: DecodePermit,
}

impl DecodeReservation {
    #[must_use]
    pub const fn reserved_bytes(&self) -> usize {
        self.permit.reserved
    }
}

impl std::fmt::Debug for DecodeReservation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DecodeReservation")
            .field("reserved_bytes", &self.reserved_bytes())
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecodeQuality {
    Full,
    FullRaw,
    EmbeddedPreview,
}

#[derive(Debug)]
pub struct DecodedTile {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Debug)]
pub struct LoadResult {
    pub request_id: u64,
    pub result: Result<DecodedImage, LoadError>,
}

#[derive(Debug)]
pub struct LoadHandle {
    request_id: u64,
    cancelled: Arc<AtomicBool>,
}

impl LoadHandle {
    #[must_use]
    pub const fn request_id(&self) -> u64 {
        self.request_id
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

impl Drop for LoadHandle {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[derive(Debug, Error)]
pub enum LoadError {
    #[error("image loading was cancelled")]
    Cancelled,
    #[error("this file format is not enabled")]
    UnsupportedFormat,
    #[error("the RAW file has no decodable embedded JPEG preview")]
    EmbeddedPreviewMissing,
    #[error("RAW development did not produce a displayable image")]
    DevelopedImageEmpty,
    #[error("could not read image: {0}")]
    Io(#[from] std::io::Error),
    #[error("could not decode image: {0}")]
    Decode(#[from] image::ImageError),
    #[error("could not decode RAW metadata: {0}")]
    Raw(#[from] rawler::RawlerError),
}

struct LoadRequest {
    request_id: u64,
    path: PathBuf,
    cancelled: Arc<AtomicBool>,
    mode: LoadMode,
    raw_options: RawDevelopOptions,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LoadMode {
    Standard,
    FullRaw,
}

pub struct ImageLoader {
    request_tx: Option<Sender<LoadRequest>>,
    result_rx: Receiver<LoadResult>,
    next_request_id: AtomicU64,
    workers: Vec<JoinHandle<()>>,
}

impl ImageLoader {
    #[must_use]
    pub fn new(worker_count: usize) -> Self {
        Self::with_budget(worker_count, DEFAULT_DECODE_BUDGET_BYTES)
    }

    #[must_use]
    pub fn with_budget(worker_count: usize, budget_bytes: usize) -> Self {
        let worker_count = worker_count.clamp(1, 8);
        let (request_tx, request_rx) = unbounded::<LoadRequest>();
        let (result_tx, result_rx) = unbounded::<LoadResult>();
        let budget = Arc::new(DecodeBudget::new(budget_bytes));
        let workers = (0..worker_count)
            .map(|index| {
                let request_rx = request_rx.clone();
                let result_tx = result_tx.clone();
                let budget = Arc::clone(&budget);
                thread::Builder::new()
                    .name(format!("image-decode-{index}"))
                    .spawn(move || worker_loop(&request_rx, &result_tx, &budget))
                    .expect("image decode worker should start")
            })
            .collect();

        Self {
            request_tx: Some(request_tx),
            result_rx,
            next_request_id: AtomicU64::new(1),
            workers,
        }
    }

    pub fn load(&self, path: impl Into<PathBuf>) -> LoadHandle {
        self.load_with_mode(
            path.into(),
            LoadMode::Standard,
            RawDevelopOptions::default(),
        )
    }

    pub fn load_full_raw(&self, path: impl Into<PathBuf>) -> LoadHandle {
        self.load_full_raw_with_options(path, RawDevelopOptions::default())
    }

    pub fn load_full_raw_with_options(
        &self,
        path: impl Into<PathBuf>,
        options: RawDevelopOptions,
    ) -> LoadHandle {
        self.load_with_mode(path.into(), LoadMode::FullRaw, options)
    }

    fn load_with_mode(
        &self,
        path: PathBuf,
        mode: LoadMode,
        raw_options: RawDevelopOptions,
    ) -> LoadHandle {
        let request_id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        let cancelled = Arc::new(AtomicBool::new(false));
        let request = LoadRequest {
            request_id,
            path,
            cancelled: Arc::clone(&cancelled),
            mode,
            raw_options,
        };
        if let Some(request_tx) = &self.request_tx {
            let _ = request_tx.send(request);
        }
        LoadHandle {
            request_id,
            cancelled,
        }
    }

    pub fn try_recv(&self) -> Result<LoadResult, TryRecvError> {
        self.result_rx.try_recv()
    }
}

impl Drop for ImageLoader {
    fn drop(&mut self) {
        self.request_tx.take();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

fn worker_loop(
    request_rx: &Receiver<LoadRequest>,
    result_tx: &Sender<LoadResult>,
    budget: &Arc<DecodeBudget>,
) {
    while let Ok(request) = request_rx.recv() {
        let full_raw = request.mode == LoadMode::FullRaw;
        let requested_bytes = if full_raw {
            full_raw_reservation_bytes(budget.capacity)
        } else {
            estimated_decode_bytes(&request.path, request.mode)
        };
        let result = budget
            .acquire(requested_bytes, &request.cancelled)
            .and_then(|permit| {
                decode_image(
                    &request.path,
                    &request.cancelled,
                    request.mode,
                    request.raw_options,
                )
                .map(|mut decoded| {
                    decoded.reservation = Some(DecodeReservation { permit });
                    decoded
                })
            });
        if result_tx
            .send(LoadResult {
                request_id: request.request_id,
                result,
            })
            .is_err()
        {
            return;
        }
    }
}

fn full_raw_reservation_bytes(capacity: usize) -> usize {
    capacity.div_ceil(MAX_PARALLEL_FULL_RAW_DECODE)
}

fn estimated_decode_bytes(path: &Path, mode: LoadMode) -> usize {
    if mode == LoadMode::FullRaw {
        return usize::MAX;
    }
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();
    if matches!(extension.to_ascii_lowercase().as_str(), "jpg" | "jpeg") {
        let dimensions = File::open(path)
            .ok()
            .and_then(|file| {
                image::ImageReader::new(BufReader::new(file))
                    .with_guessed_format()
                    .ok()
            })
            .and_then(|reader| reader.into_dimensions().ok());
        if let Some((width, height)) = dimensions {
            return (u64::from(width) * u64::from(height) * 8)
                .try_into()
                .unwrap_or(usize::MAX);
        }
    }
    RAW_PREVIEW_ESTIMATED_BYTES
}

struct DecodeBudget {
    capacity: usize,
    used: Mutex<usize>,
    changed: Condvar,
}

impl DecodeBudget {
    fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            used: Mutex::new(0),
            changed: Condvar::new(),
        }
    }

    fn acquire(
        self: &Arc<Self>,
        requested: usize,
        cancelled: &AtomicBool,
    ) -> Result<DecodePermit, LoadError> {
        let reserved = requested.min(self.capacity).max(1);
        let mut used = self.used.lock().unwrap_or_else(|error| error.into_inner());
        while *used > self.capacity.saturating_sub(reserved) {
            check_cancelled(cancelled)?;
            let (next, _) = self
                .changed
                .wait_timeout(used, Duration::from_millis(25))
                .unwrap_or_else(|error| error.into_inner());
            used = next;
        }
        check_cancelled(cancelled)?;
        *used += reserved;
        Ok(DecodePermit {
            budget: Arc::clone(self),
            reserved,
        })
    }
}

struct DecodePermit {
    budget: Arc<DecodeBudget>,
    reserved: usize,
}

impl Drop for DecodePermit {
    fn drop(&mut self) {
        let mut used = self
            .budget
            .used
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        *used = used.saturating_sub(self.reserved);
        self.budget.changed.notify_all();
    }
}

fn decode_image(
    path: &Path,
    cancelled: &AtomicBool,
    mode: LoadMode,
    raw_options: RawDevelopOptions,
) -> Result<DecodedImage, LoadError> {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();
    if matches!(extension.to_ascii_lowercase().as_str(), "jpg" | "jpeg") {
        decode_jpeg(path, cancelled)
    } else if is_raw_path(path) {
        match mode {
            LoadMode::Standard => decode_raw_preview(path, cancelled),
            LoadMode::FullRaw => decode_raw_full(path, cancelled, raw_options),
        }
    } else {
        Err(LoadError::UnsupportedFormat)
    }
}

fn decode_jpeg(path: &Path, cancelled: &AtomicBool) -> Result<DecodedImage, LoadError> {
    check_cancelled(cancelled)?;
    let started = Instant::now();
    let mut decoder = JpegDecoder::new(BufReader::new(File::open(path)?))?;
    let orientation = decoder.orientation()?;
    let icc_profile = decoder.icc_profile()?;
    let mut decoded = DynamicImage::from_decoder(decoder)?;
    if let Some(profile) = icc_profile {
        decoded = convert_icc_to_srgb(decoded, &profile);
    }
    decoded.apply_orientation(orientation);
    let rgba = decoded.into_rgba8();
    check_cancelled(cancelled)?;
    let (width, height) = rgba.dimensions();
    let (display_linear_luminance_percentiles, display_linear_rgb_medians) =
        display_linear_stats(&rgba);
    let luminance_grid = build_luminance_grid(&rgba);
    let color_grid = build_color_grid(&rgba);
    let registration_image = build_registration_image(&rgba);
    let tiles = split_into_tiles(&rgba, cancelled)?;
    Ok(DecodedImage {
        path: path.to_owned(),
        width,
        height,
        source_width: width,
        source_height: height,
        quality: DecodeQuality::Full,
        camera: None,
        lens: None,
        bit_depth: Some(8),
        capture: CaptureMetadata::default(),
        raw_recipe: None,
        raw_diagnostics: None,
        display_linear_luminance_percentiles,
        display_linear_rgb_medians,
        luminance_grid,
        color_grid,
        registration_image,
        tiles,
        decode_time: started.elapsed(),
        reservation: None,
    })
}

fn convert_icc_to_srgb(image: DynamicImage, profile: &[u8]) -> DynamicImage {
    let Ok(source_profile) = ColorProfile::new_from_slice(profile) else {
        return image;
    };
    let destination_profile = ColorProfile::new_srgb();
    let Ok(transform) = source_profile.create_transform_8bit(
        Layout::Rgb,
        &destination_profile,
        Layout::Rgb,
        TransformOptions::default(),
    ) else {
        return image;
    };
    let source = image.into_rgb8();
    let (width, height) = source.dimensions();
    let source = source.into_raw();
    let mut destination = vec![0_u8; source.len()];
    let row_bytes = width as usize * 3;
    for (source_row, destination_row) in source
        .chunks_exact(row_bytes)
        .zip(destination.chunks_exact_mut(row_bytes))
    {
        if transform.transform(source_row, destination_row).is_err() {
            return DynamicImage::ImageRgb8(
                image::RgbImage::from_raw(width, height, source)
                    .expect("decoded JPEG dimensions match its RGB storage"),
            );
        }
    }
    DynamicImage::ImageRgb8(
        image::RgbImage::from_raw(width, height, destination)
            .expect("ICC destination dimensions match its RGB storage"),
    )
}

fn decode_raw_preview(path: &Path, cancelled: &AtomicBool) -> Result<DecodedImage, LoadError> {
    check_cancelled(cancelled)?;
    let started = Instant::now();
    let mut file = File::open(path)?;
    let preview = decode_largest_embedded_jpeg_from_reader(&mut file)?
        .ok_or(LoadError::EmbeddedPreviewMissing)?;
    check_cancelled(cancelled)?;

    let source = RawSource::new(path)?;
    let decoder = get_decoder(&source)?;
    let parameters = RawDecodeParams::default();
    let raw = decoder.raw_image(&source, &parameters, true)?;
    let metadata = decoder.raw_metadata(&source, &parameters)?;
    let orientation = effective_raw_orientation(raw.orientation, metadata.exif.orientation);
    let preview = orient_preview(preview, orientation);
    let (width, height) = preview.dimensions();
    let (display_linear_luminance_percentiles, display_linear_rgb_medians) =
        display_linear_stats(&preview);
    let luminance_grid = build_luminance_grid(&preview);
    let color_grid = build_color_grid(&preview);
    let registration_image = build_registration_image(&preview);
    let source_dimensions = raw
        .crop_area
        .or(raw.active_area)
        .map_or([raw.width as u32, raw.height as u32], |crop| {
            [crop.d.w as u32, crop.d.h as u32]
        });
    let source_dimensions = oriented_dimensions(source_dimensions, orientation);
    let lens = metadata.lens.map(|lens| lens.lens_name);
    let camera = Some(format!("{} {}", metadata.make, metadata.model));
    let capture = CaptureMetadata {
        iso: metadata
            .exif
            .iso_speed
            .or(metadata.exif.iso_speed_ratings.map(u32::from))
            .or(metadata.exif.recommended_exposure_index),
        shutter: metadata.exif.exposure_time.and_then(format_shutter),
        aperture: metadata
            .exif
            .fnumber
            .and_then(|value| rational_value(value).map(|value| format!("f/{value:.1}"))),
        focal_length: metadata.exif.focal_length.and_then(|value| {
            rational_value(value).map(|value| {
                if (value - value.round()).abs() < 0.05 {
                    format!("{value:.0} mm")
                } else {
                    format!("{value:.1} mm")
                }
            })
        }),
        exposure_time_seconds: metadata.exif.exposure_time.and_then(rational_value),
        fnumber: metadata.exif.fnumber.and_then(rational_value),
    };
    let tiles = split_into_tiles(&preview, cancelled)?;

    Ok(DecodedImage {
        path: path.to_owned(),
        width,
        height,
        source_width: source_dimensions[0],
        source_height: source_dimensions[1],
        quality: DecodeQuality::EmbeddedPreview,
        camera,
        lens,
        bit_depth: Some(raw.bps),
        capture,
        raw_recipe: None,
        raw_diagnostics: None,
        display_linear_luminance_percentiles,
        display_linear_rgb_medians,
        luminance_grid,
        color_grid,
        registration_image,
        tiles,
        decode_time: started.elapsed(),
        reservation: None,
    })
}

fn decode_raw_full(
    path: &Path,
    cancelled: &AtomicBool,
    options: RawDevelopOptions,
) -> Result<DecodedImage, LoadError> {
    check_cancelled(cancelled)?;
    let started = Instant::now();
    let source = RawSource::new(path)?;
    let decoder = get_decoder(&source)?;
    let parameters = RawDecodeParams::default();
    let raw = decoder.raw_image(&source, &parameters, false)?;
    let metadata = decoder.raw_metadata(&source, &parameters)?;
    let orientation = effective_raw_orientation(raw.orientation, metadata.exif.orientation);
    check_cancelled(cancelled)?;

    let mut recipe = RawRecipe {
        display_mode: options.mode,
        white_balance_source: if raw.wb_coeffs[0].is_nan() {
            WhiteBalanceSource::Fallback
        } else {
            WhiteBalanceSource::Camera
        },
        ..RawRecipe::default()
    };
    let black_levels = raw.blacklevel.as_vec();
    let white_levels = raw.whitelevel.as_vec();
    let white_balance = raw.wb_coeffs;
    let mut developer = RawDevelop::default();
    developer.steps.retain(|step| *step != ProcessingStep::SRgb);
    let mut intermediate = developer.develop_intermediate(&raw)?;
    let mut linear_stats = None;
    if options.mode != RawDisplayMode::LinearDiagnostic {
        let stats = linear_luminance_stats(&intermediate);
        linear_stats = Some(stats);
        let mut automatic_ev = match options.mode {
            RawDisplayMode::AsShot => 0.0,
            RawDisplayMode::Reference => (inverse_shoulder(0.18) / stats[2].max(1.0e-6)).log2(),
            RawDisplayMode::PreviewMatched => {
                (inverse_shoulder(embedded_preview_linear_median(path).unwrap_or(0.18))
                    / stats[2].max(1.0e-6))
                .log2()
            }
            RawDisplayMode::LinearDiagnostic => unreachable!(),
        }
        .clamp(-2.0, 2.0);
        let highlight_limit_ev = (4.0 / stats[4].max(1.0e-6)).log2();
        automatic_ev = automatic_ev.min(highlight_limit_ev);
        recipe.automatic_exposure_ev = automatic_ev;
        recipe.comparison_match_ev = options.comparison_match_ev.clamp(-4.0, 4.0);
        recipe.tone_map = ToneMap::ReferenceSigmoid;
        recipe.highlight_method = HighlightMethod::SoftShoulder;
        apply_reference_view_transform(
            &mut intermediate,
            recipe.automatic_exposure_ev + recipe.comparison_match_ev,
        );
    }
    let developed = intermediate
        .to_dynamic_image()
        .ok_or(LoadError::DevelopedImageEmpty)?
        .into_rgba8();
    check_cancelled(cancelled)?;
    let developed = orient_preview(developed, orientation);
    let raw_diagnostics = display_diagnostics(
        &developed,
        black_levels,
        white_levels,
        white_balance,
        linear_stats,
    );
    let (display_linear_luminance_percentiles, display_linear_rgb_medians) =
        display_linear_stats(&developed);
    let luminance_grid = build_luminance_grid(&developed);
    let color_grid = build_color_grid(&developed);
    let registration_image = build_registration_image(&developed);
    let (width, height) = developed.dimensions();
    let lens = metadata.lens.map(|lens| lens.lens_name);
    let camera = Some(format!("{} {}", metadata.make, metadata.model));
    let capture = CaptureMetadata {
        iso: metadata
            .exif
            .iso_speed
            .or(metadata.exif.iso_speed_ratings.map(u32::from))
            .or(metadata.exif.recommended_exposure_index),
        shutter: metadata.exif.exposure_time.and_then(format_shutter),
        aperture: metadata
            .exif
            .fnumber
            .and_then(|value| rational_value(value).map(|value| format!("f/{value:.1}"))),
        focal_length: metadata.exif.focal_length.and_then(|value| {
            rational_value(value).map(|value| {
                if (value - value.round()).abs() < 0.05 {
                    format!("{value:.0} mm")
                } else {
                    format!("{value:.1} mm")
                }
            })
        }),
        exposure_time_seconds: metadata.exif.exposure_time.and_then(rational_value),
        fnumber: metadata.exif.fnumber.and_then(rational_value),
    };
    let tiles = split_into_tiles(&developed, cancelled)?;

    Ok(DecodedImage {
        path: path.to_owned(),
        width,
        height,
        source_width: width,
        source_height: height,
        quality: DecodeQuality::FullRaw,
        camera,
        lens,
        bit_depth: Some(raw.bps),
        capture,
        raw_recipe: Some(recipe),
        raw_diagnostics: Some(raw_diagnostics),
        display_linear_luminance_percentiles,
        display_linear_rgb_medians,
        luminance_grid,
        color_grid,
        registration_image,
        tiles,
        decode_time: started.elapsed(),
        reservation: None,
    })
}

fn embedded_preview_linear_median(path: &Path) -> Option<f32> {
    let mut file = File::open(path).ok()?;
    let preview = decode_largest_embedded_jpeg_from_reader(&mut file).ok()??;
    let mut histogram = [0_u64; 4096];
    for pixel in preview.pixels() {
        let rgb = [
            srgb_to_linear(f32::from(pixel[0]) / 255.0),
            srgb_to_linear(f32::from(pixel[1]) / 255.0),
            srgb_to_linear(f32::from(pixel[2]) / 255.0),
        ];
        let y = luminance(rgb).clamp(0.0, 1.0);
        histogram[(y * 4095.0).round() as usize] += 1;
    }
    let count = u64::from(preview.width()) * u64::from(preview.height());
    Some(generic_percentile(&histogram, count, 0.5))
}

fn linear_luminance_stats(intermediate: &Intermediate) -> [f32; 5] {
    let mut histogram = [0_u64; 4096];
    let mut count = 0_u64;
    let mut add = |value: f32| {
        let log = value.max(2.0_f32.powi(-16)).log2().clamp(-16.0, 4.0);
        let index = ((log + 16.0) / 20.0 * 4095.0).round() as usize;
        histogram[index] += 1;
        count += 1;
    };
    match intermediate {
        Intermediate::Monochrome(pixels) => pixels.data.iter().for_each(|&value| add(value)),
        Intermediate::ThreeColor(pixels) => pixels.data.iter().for_each(|&rgb| add(luminance(rgb))),
        Intermediate::FourColor(pixels) => pixels
            .data
            .iter()
            .for_each(|pixel| add(luminance([pixel[0], pixel[1], pixel[2]]))),
    }
    [0.01, 0.10, 0.50, 0.90, 0.999].map(|fraction| {
        let normalized = generic_percentile(&histogram, count, fraction);
        2.0_f32.powf(normalized * 20.0 - 16.0)
    })
}

fn generic_percentile<const N: usize>(histogram: &[u64; N], count: u64, fraction: f64) -> f32 {
    let target = (count as f64 * fraction).ceil() as u64;
    let mut accumulated = 0_u64;
    for (value, &bucket) in histogram.iter().enumerate() {
        accumulated += bucket;
        if accumulated >= target {
            return value as f32 / (N - 1) as f32;
        }
    }
    1.0
}

fn apply_reference_view_transform(intermediate: &mut Intermediate, exposure_ev: f32) {
    let gain = 2.0_f32.powf(exposure_ev);
    let map_rgb = |rgb: [f32; 3]| {
        let scaled = rgb.map(|channel| channel.max(0.0) * gain);
        let peak = scaled.into_iter().fold(0.0_f32, f32::max);
        let mapped_peak = peak / (1.0 + peak);
        let scale = if peak > 1.0e-8 {
            mapped_peak / peak
        } else {
            0.0
        };
        scaled.map(|channel| linear_to_srgb(channel * scale))
    };
    match intermediate {
        Intermediate::Monochrome(pixels) => pixels.for_each(|value| {
            let scaled = value.max(0.0) * gain;
            linear_to_srgb(scaled / (1.0 + scaled))
        }),
        Intermediate::ThreeColor(pixels) => pixels.for_each(map_rgb),
        Intermediate::FourColor(pixels) => pixels.for_each(|pixel| {
            let mapped = map_rgb([pixel[0], pixel[1], pixel[2]]);
            [mapped[0], mapped[1], mapped[2], pixel[3]]
        }),
    }
}

fn inverse_shoulder(display_linear: f32) -> f32 {
    let value = display_linear.clamp(0.0, 0.95);
    value / (1.0 - value)
}

fn luminance(rgb: [f32; 3]) -> f32 {
    0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]
}

fn linear_to_srgb(value: f32) -> f32 {
    if value <= 0.003_130_8 {
        12.92 * value
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

fn srgb_to_linear(value: f32) -> f32 {
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn display_diagnostics(
    image: &RgbaImage,
    black_levels: Vec<f32>,
    white_levels: Vec<f32>,
    white_balance: [f32; 4],
    linear_luminance_percentiles: Option<[f32; 5]>,
) -> RawDiagnostics {
    let mut histogram = [0_u64; 256];
    let mut maxima = [0_u8; 3];
    let mut clipped = 0_u64;
    for pixel in image.pixels() {
        for channel in 0..3 {
            maxima[channel] = maxima[channel].max(pixel[channel]);
        }
        if pixel[0] == u8::MAX || pixel[1] == u8::MAX || pixel[2] == u8::MAX {
            clipped += 1;
        }
        let luminance = 0.2126 * f32::from(pixel[0])
            + 0.7152 * f32::from(pixel[1])
            + 0.0722 * f32::from(pixel[2]);
        histogram[luminance.round().clamp(0.0, 255.0) as usize] += 1;
    }
    let count = u64::from(image.width()) * u64::from(image.height());
    RawDiagnostics {
        black_levels,
        white_levels,
        white_balance,
        display_channel_maxima: maxima,
        display_clipped_pixels: clipped,
        display_pixel_count: count,
        display_luminance_percentiles: [
            histogram_percentile(&histogram, count, 0.01),
            histogram_percentile(&histogram, count, 0.10),
            histogram_percentile(&histogram, count, 0.50),
            histogram_percentile(&histogram, count, 0.90),
            histogram_percentile(&histogram, count, 0.99),
        ],
        linear_luminance_percentiles,
    }
}

fn display_linear_stats(image: &RgbaImage) -> ([f32; 5], [f32; 3]) {
    let mut luminance_histogram = [0_u64; 4096];
    let mut channel_histograms = [[0_u64; 4096]; 3];
    for pixel in image.pixels() {
        let rgb = [
            srgb_to_linear(f32::from(pixel[0]) / 255.0),
            srgb_to_linear(f32::from(pixel[1]) / 255.0),
            srgb_to_linear(f32::from(pixel[2]) / 255.0),
        ];
        let y = luminance(rgb).clamp(0.0, 1.0);
        luminance_histogram[(y * 4095.0).round() as usize] += 1;
        for channel in 0..3 {
            channel_histograms[channel]
                [(rgb[channel].clamp(0.0, 1.0) * 4095.0).round() as usize] += 1;
        }
    }
    let count = u64::from(image.width()) * u64::from(image.height());
    (
        [0.01, 0.10, 0.50, 0.90, 0.99]
            .map(|fraction| generic_percentile(&luminance_histogram, count, fraction)),
        channel_histograms.map(|histogram| generic_percentile(&histogram, count, 0.5)),
    )
}

fn build_luminance_grid(image: &RgbaImage) -> LuminanceGrid {
    let grid_width = LUMINANCE_GRID_SIZE.min(image.width().max(1) as usize);
    let grid_height = LUMINANCE_GRID_SIZE.min(image.height().max(1) as usize);
    let mut sums = vec![0.0_f64; grid_width * grid_height];
    let mut counts = vec![0_u32; grid_width * grid_height];
    let step = (image.width().max(image.height()) / 512).max(1) as usize;
    for y in (0..image.height()).step_by(step) {
        for x in (0..image.width()).step_by(step) {
            let pixel = image.get_pixel(x, y);
            let rgb = [
                srgb_to_linear(f32::from(pixel[0]) / 255.0),
                srgb_to_linear(f32::from(pixel[1]) / 255.0),
                srgb_to_linear(f32::from(pixel[2]) / 255.0),
            ];
            let grid_x = (x as usize * grid_width / image.width() as usize).min(grid_width - 1);
            let grid_y = (y as usize * grid_height / image.height() as usize).min(grid_height - 1);
            let index = grid_y * grid_width + grid_x;
            sums[index] += f64::from(luminance(rgb));
            counts[index] += 1;
        }
    }
    let values = sums
        .into_iter()
        .zip(counts)
        .map(|(sum, count)| {
            if count == 0 {
                0.0
            } else {
                (sum / f64::from(count)) as f32
            }
        })
        .collect();
    LuminanceGrid {
        width: grid_width,
        height: grid_height,
        values,
    }
}

fn build_color_grid(image: &RgbaImage) -> ColorGrid {
    let grid_width = LUMINANCE_GRID_SIZE.min(image.width().max(1) as usize);
    let grid_height = LUMINANCE_GRID_SIZE.min(image.height().max(1) as usize);
    let mut sums = vec![[0.0_f64; 3]; grid_width * grid_height];
    let mut counts = vec![0_u32; grid_width * grid_height];
    let step = (image.width().max(image.height()) / 512).max(1) as usize;
    for y in (0..image.height()).step_by(step) {
        for x in (0..image.width()).step_by(step) {
            let pixel = image.get_pixel(x, y);
            let rgb = [
                srgb_to_linear(f32::from(pixel[0]) / 255.0),
                srgb_to_linear(f32::from(pixel[1]) / 255.0),
                srgb_to_linear(f32::from(pixel[2]) / 255.0),
            ];
            let grid_x = (x as usize * grid_width / image.width() as usize).min(grid_width - 1);
            let grid_y = (y as usize * grid_height / image.height() as usize).min(grid_height - 1);
            let index = grid_y * grid_width + grid_x;
            for channel in 0..3 {
                sums[index][channel] += f64::from(rgb[channel]);
            }
            counts[index] += 1;
        }
    }
    let values = sums
        .into_iter()
        .zip(counts)
        .map(|(sum, count)| {
            if count == 0 {
                [0.0; 3]
            } else {
                sum.map(|value| (value / f64::from(count)) as f32)
            }
        })
        .collect();
    ColorGrid {
        width: grid_width,
        height: grid_height,
        values,
    }
}

fn build_registration_image(image: &RgbaImage) -> RegistrationImage {
    let source_width = image.width().max(1) as usize;
    let source_height = image.height().max(1) as usize;
    let longest_side = source_width.max(source_height);
    let scale = (REGISTRATION_IMAGE_LONG_EDGE as f64 / longest_side as f64).min(1.0);
    let width = ((source_width as f64 * scale).round() as usize).max(1);
    let height = ((source_height as f64 * scale).round() as usize).max(1);
    let mut pixels = Vec::with_capacity(width * height);
    let mut histogram = [0_u64; 256];

    for target_y in 0..height {
        for target_x in 0..width {
            let source_x = ((target_x as f64 + 0.5) * source_width as f64 / width as f64 - 0.5)
                .clamp(0.0, (source_width - 1) as f64);
            let source_y = ((target_y as f64 + 0.5) * source_height as f64 / height as f64 - 0.5)
                .clamp(0.0, (source_height - 1) as f64);
            let x0 = source_x.floor() as u32;
            let y0 = source_y.floor() as u32;
            let x1 = (x0 + 1).min(image.width().saturating_sub(1));
            let y1 = (y0 + 1).min(image.height().saturating_sub(1));
            let fraction_x = (source_x - f64::from(x0)) as f32;
            let fraction_y = (source_y - f64::from(y0)) as f32;
            let luma = |x: u32, y: u32| {
                let pixel = image.get_pixel(x, y);
                0.2126 * f32::from(pixel[0])
                    + 0.7152 * f32::from(pixel[1])
                    + 0.0722 * f32::from(pixel[2])
            };
            let top = luma(x0, y0) * (1.0 - fraction_x) + luma(x1, y0) * fraction_x;
            let bottom = luma(x0, y1) * (1.0 - fraction_x) + luma(x1, y1) * fraction_x;
            let value = (top * (1.0 - fraction_y) + bottom * fraction_y)
                .round()
                .clamp(0.0, 255.0) as u8;
            pixels.push(value);
            histogram[usize::from(value)] += 1;
        }
    }

    let count = pixels.len() as u64;
    let low = (histogram_percentile(&histogram, count, 0.01) * 255.0).round() as u8;
    let high = (histogram_percentile(&histogram, count, 0.99) * 255.0).round() as u8;
    if high > low.saturating_add(4) {
        let range = u16::from(high - low);
        for pixel in &mut pixels {
            *pixel = if *pixel <= low {
                0
            } else if *pixel >= high {
                255
            } else {
                ((u16::from(*pixel - low) * 255 + range / 2) / range) as u8
            };
        }
    }

    RegistrationImage {
        width,
        height,
        pixels,
    }
}

fn histogram_percentile(histogram: &[u64; 256], count: u64, fraction: f64) -> f32 {
    let target = (count as f64 * fraction).ceil() as u64;
    let mut accumulated = 0_u64;
    for (value, &bucket) in histogram.iter().enumerate() {
        accumulated += bucket;
        if accumulated >= target {
            return value as f32 / 255.0;
        }
    }
    1.0
}

fn rational_value(value: rawler::formats::tiff::Rational) -> Option<f64> {
    (value.d != 0).then(|| f64::from(value.n) / f64::from(value.d))
}

fn format_shutter(value: rawler::formats::tiff::Rational) -> Option<String> {
    let seconds = rational_value(value)?;
    if seconds >= 1.0 {
        if (seconds - seconds.round()).abs() < 0.05 {
            Some(format!("{seconds:.0} s"))
        } else {
            Some(format!("{seconds:.1} s"))
        }
    } else if seconds > 0.0 {
        Some(format!("1/{:.0} s", (1.0 / seconds).round()))
    } else {
        None
    }
}

fn orient_preview(preview: RgbaImage, orientation: Orientation) -> RgbaImage {
    let image = DynamicImage::ImageRgba8(preview);
    match orientation {
        Orientation::Normal | Orientation::Unknown => image.into_rgba8(),
        Orientation::HorizontalFlip => image.fliph().into_rgba8(),
        Orientation::Rotate180 => image.rotate180().into_rgba8(),
        Orientation::VerticalFlip => image.flipv().into_rgba8(),
        Orientation::Transpose => image.rotate90().fliph().into_rgba8(),
        Orientation::Rotate90 => image.rotate90().into_rgba8(),
        Orientation::Transverse => image.rotate90().flipv().into_rgba8(),
        Orientation::Rotate270 => image.rotate270().into_rgba8(),
    }
}

fn effective_raw_orientation(
    decoded_orientation: Orientation,
    metadata_orientation: Option<u16>,
) -> Orientation {
    metadata_orientation
        .map(Orientation::from_u16)
        .filter(|orientation| *orientation != Orientation::Unknown)
        .unwrap_or(decoded_orientation)
}

const fn oriented_dimensions(dimensions: [u32; 2], orientation: Orientation) -> [u32; 2] {
    match orientation {
        Orientation::Transpose
        | Orientation::Rotate90
        | Orientation::Transverse
        | Orientation::Rotate270 => [dimensions[1], dimensions[0]],
        Orientation::Normal
        | Orientation::Unknown
        | Orientation::HorizontalFlip
        | Orientation::Rotate180
        | Orientation::VerticalFlip => dimensions,
    }
}

fn split_into_tiles(
    image: &RgbaImage,
    cancelled: &AtomicBool,
) -> Result<Vec<DecodedTile>, LoadError> {
    let (width, height) = image.dimensions();
    let columns = width.div_ceil(DECODE_TILE_SIZE);
    let rows = height.div_ceil(DECODE_TILE_SIZE);
    let mut tiles = Vec::with_capacity((columns * rows) as usize);

    for tile_y in 0..rows {
        check_cancelled(cancelled)?;
        for tile_x in 0..columns {
            let x = tile_x * DECODE_TILE_SIZE;
            let y = tile_y * DECODE_TILE_SIZE;
            let tile_width = DECODE_TILE_SIZE.min(width - x);
            let tile_height = DECODE_TILE_SIZE.min(height - y);
            let mut rgba = Vec::with_capacity((tile_width * tile_height * 4) as usize);
            for source_y in y..y + tile_height {
                let row_start = ((source_y * width + x) * 4) as usize;
                let row_end = row_start + (tile_width * 4) as usize;
                rgba.extend_from_slice(&image.as_raw()[row_start..row_end]);
            }
            tiles.push(DecodedTile {
                x,
                y,
                width: tile_width,
                height: tile_height,
                rgba,
            });
        }
    }
    Ok(tiles)
}

fn check_cancelled(cancelled: &AtomicBool) -> Result<(), LoadError> {
    if cancelled.load(Ordering::Acquire) {
        Err(LoadError::Cancelled)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{io::Cursor, sync::mpsc};

    use image::{ExtendedColorType, ImageEncoder, Rgba, codecs::jpeg::JpegEncoder};

    use super::*;

    fn encode_test_jpeg(width: u32, height: u32, rgb: &[u8]) -> Vec<u8> {
        let mut encoded = Cursor::new(Vec::new());
        JpegEncoder::new_with_quality(&mut encoded, 95)
            .write_image(rgb, width, height, ExtendedColorType::Rgb8)
            .expect("test JPEG encodes");
        encoded.into_inner()
    }

    fn test_jpeg_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("imagecompare-{label}-{}.jpg", std::process::id()))
    }

    #[test]
    fn large_images_are_split_into_bounded_edge_tiles() {
        let image = RgbaImage::from_pixel(1025, 513, Rgba([1, 2, 3, 255]));
        let cancelled = AtomicBool::new(false);
        let tiles = split_into_tiles(&image, &cancelled).expect("tiling should succeed");

        assert_eq!(tiles.len(), 6);
        assert_eq!((tiles[0].width, tiles[0].height), (512, 512));
        assert_eq!((tiles[2].width, tiles[2].height), (1, 512));
        assert_eq!((tiles[5].width, tiles[5].height), (1, 1));
        assert_eq!(
            tiles.iter().map(|tile| tile.rgba.len()).sum::<usize>(),
            1025 * 513 * 4
        );
    }

    #[test]
    fn cancellation_stops_tiling_before_allocation() {
        let image = RgbaImage::new(1024, 1024);
        let cancelled = AtomicBool::new(true);
        assert!(matches!(
            split_into_tiles(&image, &cancelled),
            Err(LoadError::Cancelled)
        ));
    }

    #[test]
    fn decode_budget_blocks_until_capacity_is_released() {
        let budget = Arc::new(DecodeBudget::new(100));
        let cancelled = Arc::new(AtomicBool::new(false));
        let first = budget.acquire(80, &cancelled).expect("first permit");
        let (tx, rx) = mpsc::channel();
        let waiting_budget = Arc::clone(&budget);
        let waiting_cancelled = Arc::clone(&cancelled);
        let worker = thread::spawn(move || {
            let permit = waiting_budget
                .acquire(30, &waiting_cancelled)
                .expect("waiting permit");
            tx.send(permit).expect("receiver should exist");
        });

        assert!(rx.recv_timeout(Duration::from_millis(40)).is_err());
        drop(first);
        let second = rx
            .recv_timeout(Duration::from_secs(1))
            .expect("capacity release should wake waiter");
        drop(second);
        worker.join().expect("worker should finish");
    }

    #[test]
    fn decode_budget_wait_can_be_cancelled() {
        let budget = Arc::new(DecodeBudget::new(100));
        let cancelled = Arc::new(AtomicBool::new(false));
        let first = budget.acquire(100, &cancelled).expect("first permit");
        let waiting_budget = Arc::clone(&budget);
        let waiting_cancelled = Arc::clone(&cancelled);
        let worker = thread::spawn(move || waiting_budget.acquire(1, &waiting_cancelled));

        thread::sleep(Duration::from_millis(40));
        cancelled.store(true, Ordering::Release);
        assert!(matches!(
            worker.join().expect("worker should finish"),
            Err(LoadError::Cancelled)
        ));
        drop(first);
    }

    #[test]
    fn exposure_values_are_formatted_for_compact_titles() {
        use rawler::formats::tiff::Rational;

        assert_eq!(
            format_shutter(Rational { n: 1, d: 500 }),
            Some("1/500 s".to_owned())
        );
        assert_eq!(
            format_shutter(Rational { n: 3, d: 2 }),
            Some("1.5 s".to_owned())
        );
        assert_eq!(format_shutter(Rational { n: 1, d: 0 }), None);
    }

    #[test]
    fn capture_ev_accounts_for_aperture_shutter_and_iso() {
        let metadata = CaptureMetadata {
            iso: Some(100),
            exposure_time_seconds: Some(1.0 / 125.0),
            fnumber: Some(4.0),
            ..CaptureMetadata::default()
        };
        assert!(
            (metadata.exposure_value_ev100().expect("complete exposure") - 10.966).abs() < 0.001
        );

        let one_stop_darker = CaptureMetadata {
            exposure_time_seconds: Some(1.0 / 250.0),
            ..metadata
        };
        assert!(
            (one_stop_darker
                .exposure_value_ev100()
                .expect("complete exposure")
                - 11.966)
                .abs()
                < 0.001
        );
    }

    #[test]
    fn raw_path_detection_is_case_insensitive() {
        assert!(is_raw_path(Path::new("capture.ORF")));
        assert!(is_raw_path(Path::new("capture.cr3")));
        assert!(!is_raw_path(Path::new("capture.jpg")));
    }

    #[test]
    fn supported_extensions_are_unique_and_include_jpeg_and_raw() {
        let extensions = supported_extensions();
        assert!(extensions.contains(&"jpg"));
        assert!(extensions.contains(&"jpeg"));
        assert!(
            extensions
                .iter()
                .any(|extension| extension.eq_ignore_ascii_case("orf"))
        );
        let mut unique = extensions
            .iter()
            .map(|extension| extension.to_ascii_lowercase())
            .collect::<Vec<_>>();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), extensions.len());
    }

    #[test]
    fn loader_worker_decodes_jpeg_and_holds_its_memory_reservation() {
        let path = test_jpeg_path("worker");
        let pixels = [
            255, 0, 0, 0, 255, 0, //
            0, 0, 255, 255, 255, 255,
        ];
        std::fs::write(&path, encode_test_jpeg(2, 2, &pixels)).expect("fixture writes");

        let loader = ImageLoader::with_budget(2, 1_024);
        let handle = loader.load(&path);
        let deadline = Instant::now() + Duration::from_secs(2);
        let load_result = loop {
            match loader.try_recv() {
                Ok(result) => break result,
                Err(TryRecvError::Empty) if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("loader did not return a result: {error}"),
            }
        };

        assert_eq!(load_result.request_id, handle.request_id());
        let mut decoded = load_result.result.expect("JPEG decodes");
        assert_eq!((decoded.width, decoded.height), (2, 2));
        assert_eq!(decoded.quality, DecodeQuality::Full);
        assert_eq!(decoded.byte_len(), 2 * 2 * 4);
        assert_eq!(
            (decoded.luminance_grid.width, decoded.luminance_grid.height),
            (2, 2)
        );
        let reservation = decoded
            .take_reservation()
            .expect("worker reserves decode memory");
        assert_eq!(reservation.reserved_bytes(), 2 * 2 * 8);
        assert!(decoded.take_reservation().is_none());

        drop(reservation);
        drop(handle);
        drop(loader);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn unsupported_extensions_are_rejected_before_file_access() {
        assert!(matches!(
            decode_image(
                Path::new("missing.png"),
                &AtomicBool::new(false),
                LoadMode::Standard,
                RawDevelopOptions::default(),
            ),
            Err(LoadError::UnsupportedFormat)
        ));
    }

    #[test]
    fn jpeg_exif_orientation_is_applied_before_tiling() {
        let mut exif = vec![
            b'I', b'I', 42, 0, 8, 0, 0, 0, // little-endian TIFF header
            1, 0, // one IFD entry
            0x12, 0x01, // Orientation tag
            3, 0, // SHORT
            1, 0, 0, 0, // one value
            6, 0, 0, 0, // rotate 90 degrees
            0, 0, 0, 0, // no next IFD
        ];
        let mut encoded = Cursor::new(Vec::new());
        let mut encoder = JpegEncoder::new_with_quality(&mut encoded, 95);
        encoder
            .set_exif_metadata(std::mem::take(&mut exif))
            .expect("JPEG supports EXIF");
        encoder
            .write_image(&[255, 0, 0, 0, 0, 255], 2, 1, ExtendedColorType::Rgb8)
            .expect("fixture encodes");

        let path = std::env::temp_dir().join(format!(
            "imagecompare-orientation-{}.jpg",
            std::process::id()
        ));
        std::fs::write(&path, encoded.into_inner()).expect("fixture writes");
        let decoded = decode_jpeg(&path, &AtomicBool::new(false)).expect("fixture decodes");
        let _ = std::fs::remove_file(path);
        assert_eq!((decoded.width, decoded.height), (1, 2));
    }

    #[test]
    fn raw_recipe_keeps_comparison_exposure_separate() {
        let recipe = RawRecipe::default();
        assert_eq!(recipe.version, RAW_RECIPE_VERSION);
        assert_eq!(recipe.baseline_exposure_ev, 0.0);
        assert_eq!(recipe.automatic_exposure_ev, 0.0);
        assert_eq!(recipe.comparison_match_ev, 0.0);
        assert_eq!(recipe.tone_map, ToneMap::SrgbTransferOnly);
    }

    #[test]
    fn histogram_percentiles_are_deterministic() {
        let mut histogram = [0_u64; 256];
        histogram[25] = 1;
        histogram[128] = 2;
        histogram[230] = 1;
        assert!((histogram_percentile(&histogram, 4, 0.5) - 128.0 / 255.0).abs() < 1e-6);
        assert!((histogram_percentile(&histogram, 4, 0.99) - 230.0 / 255.0).abs() < 1e-6);
    }

    #[test]
    fn display_linear_stats_report_per_channel_medians() {
        let image = RgbaImage::from_pixel(4, 4, Rgba([128, 64, 32, 255]));
        let (_, medians) = display_linear_stats(&image);
        let expected = [128.0, 64.0, 32.0].map(|value| srgb_to_linear(value / 255.0));

        for (actual, expected) in medians.into_iter().zip(expected) {
            assert!((actual - expected).abs() < 1.0 / 4095.0);
        }
    }

    #[test]
    fn color_grid_preserves_linear_rgb_channels() {
        let image = RgbaImage::from_pixel(4, 4, Rgba([128, 64, 32, 255]));
        let grid = build_color_grid(&image);
        let expected = [128.0, 64.0, 32.0].map(|value| srgb_to_linear(value / 255.0));

        assert_eq!((grid.width, grid.height), (4, 4));
        assert_eq!(grid.values.len(), 16);
        for sample in grid.values {
            for (actual, expected) in sample.into_iter().zip(expected) {
                assert!((actual - expected).abs() < 1.0e-6);
            }
        }
    }

    #[test]
    fn display_diagnostics_and_luminance_grid_track_known_pixels() {
        let image = RgbaImage::from_vec(
            2,
            2,
            vec![
                0, 0, 0, 255, //
                255, 255, 255, 255, //
                255, 0, 0, 255, //
                0, 255, 0, 255,
            ],
        )
        .expect("dimensions match pixels");
        let diagnostics = display_diagnostics(
            &image,
            vec![64.0],
            vec![4_095.0],
            [2.0, 1.0, 1.5, f32::NAN],
            Some([0.1; 5]),
        );
        assert_eq!(diagnostics.display_channel_maxima, [255, 255, 255]);
        assert_eq!(diagnostics.display_clipped_pixels, 3);
        assert_eq!(diagnostics.display_pixel_count, 4);
        assert_eq!(diagnostics.linear_luminance_percentiles, Some([0.1; 5]));

        let grid = build_luminance_grid(&image);
        assert_eq!((grid.width, grid.height), (2, 2));
        for (actual, expected) in grid.values.iter().zip([0.0, 1.0, 0.2126, 0.7152]) {
            assert!((actual - expected).abs() < 1.0e-4);
        }
    }

    #[test]
    fn registration_image_is_bounded_aspect_preserving_and_contrast_stretched() {
        let image = RgbaImage::from_fn(1_200, 600, |x, _| {
            let value = if x < 12 {
                10
            } else if x >= 1_188 {
                220
            } else {
                40 + ((x - 12) * 80 / 1_176) as u8
            };
            Rgba([value, value, value, 255])
        });

        let registration = build_registration_image(&image);

        assert_eq!(
            (registration.width, registration.height),
            (REGISTRATION_IMAGE_LONG_EDGE, 320)
        );
        assert_eq!(
            registration.pixels.len(),
            registration.width * registration.height
        );
        assert_eq!(registration.pixels.iter().copied().min(), Some(0));
        assert_eq!(registration.pixels.iter().copied().max(), Some(255));
    }

    #[test]
    fn registration_image_does_not_upscale_small_sources() {
        let image = RgbaImage::from_pixel(24, 16, Rgba([80, 80, 80, 255]));
        let registration = build_registration_image(&image);

        assert_eq!((registration.width, registration.height), (24, 16));
        assert_eq!(registration.pixels, vec![80; 24 * 16]);
    }

    #[test]
    fn srgb_transfer_functions_round_trip_reference_values() {
        for linear in [0.0_f32, 0.001, 0.003_130_8, 0.18, 0.5, 1.0] {
            let round_trip = srgb_to_linear(linear_to_srgb(linear));
            assert!((round_trip - linear).abs() < 1.0e-5);
        }
    }

    #[test]
    fn every_raw_orientation_preserves_pixels_and_expected_dimensions() {
        let source = RgbaImage::from_fn(2, 3, |x, y| {
            Rgba([(y * 2 + x) as u8, x as u8, y as u8, 255])
        });
        let mut expected_pixels = source.pixels().map(|pixel| pixel.0).collect::<Vec<_>>();
        expected_pixels.sort_unstable();

        for (orientation, expected_size) in [
            (Orientation::Normal, (2, 3)),
            (Orientation::Unknown, (2, 3)),
            (Orientation::HorizontalFlip, (2, 3)),
            (Orientation::Rotate180, (2, 3)),
            (Orientation::VerticalFlip, (2, 3)),
            (Orientation::Transpose, (3, 2)),
            (Orientation::Rotate90, (3, 2)),
            (Orientation::Transverse, (3, 2)),
            (Orientation::Rotate270, (3, 2)),
        ] {
            let oriented = orient_preview(source.clone(), orientation);
            assert_eq!(oriented.dimensions(), expected_size);
            let mut actual_pixels = oriented.pixels().map(|pixel| pixel.0).collect::<Vec<_>>();
            actual_pixels.sort_unstable();
            assert_eq!(actual_pixels, expected_pixels);
        }
    }

    #[test]
    fn raw_exif_orientation_overrides_the_decoder_default() {
        assert_eq!(
            effective_raw_orientation(Orientation::Normal, Some(6)),
            Orientation::Rotate90
        );
        assert_eq!(
            effective_raw_orientation(Orientation::Rotate180, None),
            Orientation::Rotate180
        );
        assert_eq!(
            effective_raw_orientation(Orientation::Rotate270, Some(0)),
            Orientation::Rotate270
        );
        assert_eq!(
            oriented_dimensions([4_000, 3_000], Orientation::Rotate90),
            [3_000, 4_000]
        );
        assert_eq!(
            oriented_dimensions([4_000, 3_000], Orientation::Rotate180),
            [4_000, 3_000]
        );
    }

    #[test]
    fn shoulder_is_monotonic_and_invertible() {
        for display in [0.01_f32, 0.18, 0.5, 0.9] {
            let scene = inverse_shoulder(display);
            let mapped = scene / (1.0 + scene);
            assert!((mapped - display).abs() < 1.0e-6);
        }
    }

    #[test]
    fn raw_development_defaults_to_neutral_base_render() {
        let options = RawDevelopOptions::default();
        assert_eq!(options.mode, RawDisplayMode::AsShot);
        assert_eq!(options.comparison_match_ev, 0.0);
    }

    #[test]
    fn full_raw_reservations_allow_two_developments_but_bound_a_third() {
        let budget = Arc::new(DecodeBudget::new(100));
        let cancelled = Arc::new(AtomicBool::new(false));
        let reservation = full_raw_reservation_bytes(budget.capacity);
        assert_eq!(reservation, 50);
        let first = budget
            .acquire(reservation, &cancelled)
            .expect("first full RAW permit");
        let second = budget
            .acquire(reservation, &cancelled)
            .expect("second full RAW permit");
        let (tx, rx) = mpsc::channel();
        let waiting_budget = Arc::clone(&budget);
        let waiting_cancelled = Arc::clone(&cancelled);
        let worker = thread::spawn(move || {
            let permit = waiting_budget
                .acquire(reservation, &waiting_cancelled)
                .expect("third full RAW permit");
            tx.send(permit).expect("receiver should exist");
        });

        assert!(rx.recv_timeout(Duration::from_millis(40)).is_err());
        drop(first);
        let third = rx
            .recv_timeout(Duration::from_secs(1))
            .expect("one release should wake the third full RAW");
        drop(second);
        drop(third);
        worker.join().expect("worker should finish");
    }
}
