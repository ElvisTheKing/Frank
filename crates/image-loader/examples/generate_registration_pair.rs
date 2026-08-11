#![forbid(unsafe_code)]

use std::{
    env,
    error::Error,
    path::{Path, PathBuf},
};

use image::{ImageBuffer, ImageFormat, Rgb};

const WIDTH: u32 = 640;
const HEIGHT: u32 = 480;
const SEED: u32 = 7;

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = env::args_os().skip(1);
    let reference_path = arguments
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("dist/registration-reference.jpg"));
    let target_path = arguments
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("dist/registration-target.jpg"));

    save_transformed(&reference_path, 1.0, 0.0, 0.0, 1.0)?;
    save_transformed(&target_path, 1.08, 0.05, -0.04, 0.92)?;
    println!(
        "wrote deterministic registration pair to {} and {}",
        reference_path.display(),
        target_path.display()
    );
    Ok(())
}

fn save_transformed(
    path: &Path,
    mapping_scale: f64,
    translation_x: f64,
    translation_y: f64,
    exposure: f64,
) -> Result<(), image::ImageError> {
    let image = ImageBuffer::from_fn(WIDTH, HEIGHT, |x, y| {
        let target_x = (f64::from(x) + 0.5) / f64::from(WIDTH);
        let target_y = (f64::from(y) + 0.5) / f64::from(HEIGHT);
        let reference_x = 0.5 + (target_x - 0.5 - translation_x) / mapping_scale;
        let reference_y = 0.5 + (target_y - 0.5 - translation_y) / mapping_scale;
        let value = (f64::from(pattern(reference_x, reference_y)) * exposure)
            .round()
            .clamp(0.0, 255.0) as u8;
        Rgb([value, (u16::from(value) * 4 / 5) as u8, 255 - value / 2])
    });
    image.save_with_format(path, ImageFormat::Jpeg)
}

fn pattern(x: f64, y: f64) -> u8 {
    let checker = (((x * 13.0).floor() as i32 + (y * 11.0).floor() as i32) & 1) as f64;
    let rings = (((x - 0.31).hypot(y - 0.38) * 45.0).sin() * 0.5 + 0.5) * 55.0;
    let diagonal = if (y - (0.18 + x * 0.47)).abs() < 0.018 {
        90.0
    } else {
        0.0
    };
    let boxes = if (0.58..0.78).contains(&x) && (0.22..0.47).contains(&y) {
        75.0
    } else {
        0.0
    };
    let hash = ((x * 97.0).floor() as u32).wrapping_mul(73_856_093)
        ^ ((y * 89.0).floor() as u32).wrapping_mul(19_349_663)
        ^ SEED.wrapping_mul(83_492_791);
    (25.0 + checker * 45.0 + rings + diagonal + boxes + f64::from(hash & 31))
        .round()
        .clamp(0.0, 255.0) as u8
}
