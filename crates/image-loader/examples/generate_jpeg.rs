#![forbid(unsafe_code)]

use std::{env, error::Error, path::PathBuf};

use image::{ImageBuffer, ImageFormat, Rgb};

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = env::args_os().skip(1);
    let output = arguments
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("dist/test-pattern.jpg"));
    let width = parse_dimension(arguments.next(), 4096)?;
    let height = parse_dimension(arguments.next(), 3072)?;

    let image = ImageBuffer::from_fn(width, height, |x, y| {
        let checker = (((x / 128) + (y / 128)) % 2) as u8;
        Rgb([
            ((x as u64 * 255) / u64::from(width.max(1))) as u8,
            ((y as u64 * 255) / u64::from(height.max(1))) as u8,
            if checker == 0 { 48 } else { 208 },
        ])
    });
    image.save_with_format(&output, ImageFormat::Jpeg)?;
    println!("wrote {}×{} JPEG to {}", width, height, output.display());
    Ok(())
}

fn parse_dimension(
    argument: Option<std::ffi::OsString>,
    default: u32,
) -> Result<u32, Box<dyn Error>> {
    match argument {
        Some(value) => {
            let value = value.to_str().ok_or("dimension must contain valid UTF-8")?;
            Ok(value.parse::<u32>()?.max(1))
        }
        None => Ok(default),
    }
}
