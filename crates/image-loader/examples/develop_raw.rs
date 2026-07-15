#![forbid(unsafe_code)]

use std::{env, error::Error, path::PathBuf, thread, time::Duration};

use image_loader::{DecodeQuality, DecodedImage, ImageLoader, LoadHandle};

fn main() -> Result<(), Box<dyn Error>> {
    let path = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("pass one RAW path")?;
    let loader = ImageLoader::new(1);
    let preview = wait_for_image(&loader, loader.load(&path))?;
    let preview_luminance = luminance_stats(&preview);
    drop(preview);
    let image = wait_for_image(&loader, loader.load_full_raw(&path))?;

    if image.quality != DecodeQuality::FullRaw {
        return Err("loader returned a non-RAW-full result".into());
    }
    let full_luminance = luminance_stats(&image);
    if let (Some(recipe), Some(diagnostics)) = (&image.raw_recipe, &image.raw_diagnostics) {
        println!(
            "recipe v{} {} WB {:?} {:?} black {:?} white {:?} display max {:?} clipped {}/{} ({:.4}%) display p01/p10/p50/p90/p99 {:?} linear {:?} baseline/auto/match {:+.2}/{:+.2}/{:+.2} EV tone {:?} highlights {:?}",
            recipe.version,
            recipe.developer,
            recipe.white_balance_source,
            diagnostics.white_balance,
            diagnostics.black_levels,
            diagnostics.white_levels,
            diagnostics.display_channel_maxima,
            diagnostics.display_clipped_pixels,
            diagnostics.display_pixel_count,
            diagnostics.display_clipped_pixels as f64 * 100.0
                / diagnostics.display_pixel_count.max(1) as f64,
            diagnostics.display_luminance_percentiles,
            diagnostics.linear_luminance_percentiles,
            recipe.baseline_exposure_ev,
            recipe.automatic_exposure_ev,
            recipe.comparison_match_ev,
            recipe.tone_map,
            recipe.highlight_method,
        );
    }
    println!(
        "{}\t{}×{}\t{} tiles\t{:.1} MiB tiles\t{:.1} ms\tpreview mean/p10/p50/p90 {:.3}/{:.3}/{:.3}/{:.3}\tfull {:.3}/{:.3}/{:.3}/{:.3}\tmedian {:+.2} EV",
        path.display(),
        image.width,
        image.height,
        image.tiles.len(),
        image.byte_len() as f64 / (1024.0 * 1024.0),
        image.decode_time.as_secs_f64() * 1000.0,
        preview_luminance.mean,
        preview_luminance.p10,
        preview_luminance.p50,
        preview_luminance.p90,
        full_luminance.mean,
        full_luminance.p10,
        full_luminance.p50,
        full_luminance.p90,
        (preview_luminance.p50 / full_luminance.p50.max(f64::MIN_POSITIVE)).log2()
    );
    Ok(())
}

fn wait_for_image(
    loader: &ImageLoader,
    handle: LoadHandle,
) -> Result<DecodedImage, Box<dyn Error>> {
    loop {
        match loader.try_recv() {
            Ok(result) if result.request_id == handle.request_id() => {
                return Ok(result.result?);
            }
            Ok(_) => {}
            Err(crossbeam_channel::TryRecvError::Empty) => thread::sleep(Duration::from_millis(10)),
            Err(crossbeam_channel::TryRecvError::Disconnected) => {
                return Err("image loader disconnected".into());
            }
        }
    }
}

struct LuminanceStats {
    mean: f64,
    p10: f64,
    p50: f64,
    p90: f64,
}

fn luminance_stats(image: &DecodedImage) -> LuminanceStats {
    let mut total = 0_f64;
    let mut count = 0_u64;
    let mut histogram = [0_u64; 256];
    for tile in &image.tiles {
        for pixel in tile.rgba.chunks_exact(4) {
            let luminance = 0.2126 * f64::from(pixel[0])
                + 0.7152 * f64::from(pixel[1])
                + 0.0722 * f64::from(pixel[2]);
            total += luminance;
            histogram[luminance.round().clamp(0.0, 255.0) as usize] += 1;
            count += 1;
        }
    }
    LuminanceStats {
        mean: total / count.max(1) as f64 / 255.0,
        p10: percentile(&histogram, count, 0.1),
        p50: percentile(&histogram, count, 0.5),
        p90: percentile(&histogram, count, 0.9),
    }
}

fn percentile(histogram: &[u64; 256], count: u64, fraction: f64) -> f64 {
    let target = (count as f64 * fraction).ceil() as u64;
    let mut accumulated = 0_u64;
    for (value, &bucket) in histogram.iter().enumerate() {
        accumulated += bucket;
        if accumulated >= target {
            return value as f64 / 255.0;
        }
    }
    1.0
}
