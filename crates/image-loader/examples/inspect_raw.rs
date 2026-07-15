#![forbid(unsafe_code)]

use std::{
    env,
    error::Error,
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

use rawler::{decoders::RawDecodeParams, get_decoder, rawsource::RawSource};

fn main() -> Result<(), Box<dyn Error>> {
    let inputs: Vec<PathBuf> = env::args_os().skip(1).map(PathBuf::from).collect();
    if inputs.is_empty() {
        return Err("pass one or more RAW files or directories".into());
    }

    for path in expand_inputs(&inputs)? {
        match inspect(&path) {
            Ok(summary) => println!("{summary}"),
            Err(error) => eprintln!("{}\tERROR\t{error}", path.display()),
        }
    }
    Ok(())
}

fn inspect(path: &Path) -> Result<String, Box<dyn Error>> {
    let embedded_started = Instant::now();
    let embedded = image_loader::embedded_preview_info_from_reader(&mut fs::File::open(path)?)?;
    let embedded_time = embedded_started.elapsed();
    let source = RawSource::new(path)?;
    let decoder = get_decoder(&source)?;
    let parameters = RawDecodeParams::default();
    let raw = decoder.raw_image(&source, &parameters, true)?;
    let metadata = decoder.raw_metadata(&source, &parameters)?;
    let preview_started = Instant::now();
    let preview = decoder.preview_image(&source, &parameters)?;
    let preview_time = preview_started.elapsed();
    let preview = preview.map_or_else(
        || "none".to_owned(),
        |preview| format!("{}×{}", preview.width(), preview.height()),
    );
    let embedded = embedded.map_or_else(
        || "none".to_owned(),
        |preview| {
            format!(
                "{}×{} @{}+{}",
                preview.width, preview.height, preview.offset, preview.length
            )
        },
    );
    let crop = raw
        .crop_area
        .map_or_else(|| "none".to_owned(), |crop| format!("{crop:?}"));
    let lens = metadata
        .lens
        .as_ref()
        .map_or_else(|| "unknown".to_owned(), |lens| format!("{lens:?}"));
    let iso = metadata
        .exif
        .iso_speed
        .or(metadata.exif.iso_speed_ratings.map(u32::from))
        .or(metadata.exif.recommended_exposure_index)
        .map_or_else(|| "unknown".to_owned(), |value| value.to_string());
    let shutter = metadata
        .exif
        .exposure_time
        .map_or_else(|| "unknown".to_owned(), |value| value.to_string());
    let aperture = metadata
        .exif
        .fnumber
        .map_or_else(|| "unknown".to_owned(), |value| value.to_string());
    let focal_length = metadata
        .exif
        .focal_length
        .map_or_else(|| "unknown".to_owned(), |value| value.to_string());

    Ok(format!(
        "{}\t{} {}\traw={}×{}\t{} bit\tcrop={}\torientation={:?}\tISO={}\tshutter={}\taperture={}\tfocal={}\trawler-preview={} ({:.1} ms)\tembedded={} ({:.1} ms)\tlens={}",
        path.display(),
        metadata.make,
        metadata.model,
        raw.width,
        raw.height,
        raw.bps,
        crop,
        raw.orientation,
        iso,
        shutter,
        aperture,
        focal_length,
        preview,
        preview_time.as_secs_f64() * 1000.0,
        embedded,
        embedded_time.as_secs_f64() * 1000.0,
        lens
    ))
}

fn expand_inputs(inputs: &[PathBuf]) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let mut files = Vec::new();
    for input in inputs {
        if input.is_dir() {
            for entry in fs::read_dir(input)? {
                let path = entry?.path();
                if path.is_file() && is_raw(&path) {
                    files.push(path);
                }
            }
        } else {
            files.push(input.clone());
        }
    }
    files.sort();
    Ok(files)
}

fn is_raw(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            rawler::decoders::supported_extensions()
                .iter()
                .any(|supported| extension.eq_ignore_ascii_case(supported))
        })
}
