use std::io::{self, BufReader, Cursor, Read, Seek, SeekFrom};

use image::{ImageFormat, ImageReader, RgbaImage};
use memchr::memmem;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EmbeddedPreviewInfo {
    pub offset: u64,
    pub length: u64,
    pub width: u32,
    pub height: u32,
}

#[must_use]
pub fn embedded_preview_info(bytes: &[u8]) -> Option<EmbeddedPreviewInfo> {
    jpeg_candidates(bytes)
        .into_iter()
        .max_by_key(|candidate| u64::from(candidate.width) * u64::from(candidate.height))
}

pub fn decode_largest_embedded_jpeg(bytes: &[u8]) -> Result<Option<RgbaImage>, image::ImageError> {
    let Some(info) = embedded_preview_info(bytes) else {
        return Ok(None);
    };
    let start = info.offset as usize;
    let end = start + info.length as usize;
    let jpeg = &bytes[start..end];
    image::load_from_memory_with_format(jpeg, ImageFormat::Jpeg)
        .map(|image| Some(image.into_rgba8()))
}

/// Finds the largest embedded JPEG without copying the source file into memory.
pub fn embedded_preview_info_from_reader<R>(
    reader: &mut R,
) -> Result<Option<EmbeddedPreviewInfo>, image::ImageError>
where
    R: Read + Seek,
{
    let metadata_end = orf_metadata_end(reader)?;
    if let Some(metadata_end) = metadata_end
        && let Some(preview) = orf_preview_descriptor(reader, metadata_end)?
    {
        return Ok(Some(preview));
    }
    reader.seek(SeekFrom::Start(0))?;
    let ranges = if let Some(metadata_end) = metadata_end {
        scan_jpeg_ranges(&mut reader.take(metadata_end))?
    } else {
        scan_jpeg_ranges(reader)?
    };
    let mut largest = None;
    for (offset, length) in ranges {
        let window = ReadWindow::new(reader, offset, length)?;
        let dimensions =
            ImageReader::with_format(BufReader::new(window), ImageFormat::Jpeg).into_dimensions();
        let Ok((width, height)) = dimensions else {
            continue;
        };
        let candidate = EmbeddedPreviewInfo {
            offset,
            length,
            width,
            height,
        };
        if largest.is_none_or(|current: EmbeddedPreviewInfo| {
            u64::from(width) * u64::from(height)
                > u64::from(current.width) * u64::from(current.height)
        }) {
            largest = Some(candidate);
        }
    }
    Ok(largest)
}

fn orf_preview_descriptor<R>(
    reader: &mut R,
    metadata_end: u64,
) -> Result<Option<EmbeddedPreviewInfo>, image::ImageError>
where
    R: Read + Seek,
{
    const SEARCH_BYTES: u64 = 64 * 1024;
    const DESCRIPTOR_VERSION: &[u8; 4] = b"0100";
    const DESCRIPTOR_BYTES: usize = 16;

    reader.seek(SeekFrom::Start(0))?;
    let search_length = usize::try_from(metadata_end.min(SEARCH_BYTES)).unwrap_or(64 * 1024);
    let mut prefix = vec![0_u8; search_length];
    reader.read_exact(&mut prefix)?;
    let mut largest = None;
    for descriptor in memmem::find_iter(&prefix, DESCRIPTOR_VERSION) {
        if descriptor + DESCRIPTOR_BYTES > prefix.len() {
            continue;
        }
        let valid = u32::from_le_bytes(prefix[descriptor + 4..descriptor + 8].try_into().unwrap());
        let offset = u64::from(u32::from_le_bytes(
            prefix[descriptor + 8..descriptor + 12].try_into().unwrap(),
        ));
        let length = u64::from(u32::from_le_bytes(
            prefix[descriptor + 12..descriptor + 16].try_into().unwrap(),
        ));
        if valid != 1
            || length < 4
            || offset >= metadata_end
            || offset.saturating_add(length) > metadata_end
        {
            continue;
        }
        let window = ReadWindow::new(reader, offset, length)?;
        let dimensions =
            ImageReader::with_format(BufReader::new(window), ImageFormat::Jpeg).into_dimensions();
        let Ok((width, height)) = dimensions else {
            continue;
        };
        let candidate = EmbeddedPreviewInfo {
            offset,
            length,
            width,
            height,
        };
        if largest.is_none_or(|current: EmbeddedPreviewInfo| {
            u64::from(width) * u64::from(height)
                > u64::from(current.width) * u64::from(current.height)
        }) {
            largest = Some(candidate);
        }
    }
    Ok(largest)
}

/// OM System ORFs place previews in the metadata region before the raw strip.
/// Reading the root strip offset avoids streaming through tens of megabytes of
/// sensor data merely to prove that no later JPEG exists.
fn orf_metadata_end<R>(reader: &mut R) -> io::Result<Option<u64>>
where
    R: Read + Seek,
{
    const ORF_HEADER: &[u8; 4] = b"IIRO";
    const STRIP_OFFSETS: u16 = 0x0111;
    const TIFF_LONG: u16 = 4;

    reader.seek(SeekFrom::Start(0))?;
    let mut header = [0_u8; 8];
    reader.read_exact(&mut header)?;
    if &header[..4] != ORF_HEADER {
        return Ok(None);
    }
    let ifd_offset = u64::from(u32::from_le_bytes(header[4..8].try_into().unwrap()));
    reader.seek(SeekFrom::Start(ifd_offset))?;
    let mut count = [0_u8; 2];
    reader.read_exact(&mut count)?;
    let count = u16::from_le_bytes(count);
    for _ in 0..count {
        let mut entry = [0_u8; 12];
        reader.read_exact(&mut entry)?;
        let tag = u16::from_le_bytes(entry[0..2].try_into().unwrap());
        let field_type = u16::from_le_bytes(entry[2..4].try_into().unwrap());
        let value_count = u32::from_le_bytes(entry[4..8].try_into().unwrap());
        if tag == STRIP_OFFSETS && field_type == TIFF_LONG && value_count == 1 {
            let raw_offset = u64::from(u32::from_le_bytes(entry[8..12].try_into().unwrap()));
            let file_length = reader.seek(SeekFrom::End(0))?;
            return Ok((raw_offset > ifd_offset && raw_offset <= file_length).then_some(raw_offset));
        }
    }
    Ok(None)
}

/// Decodes only the selected embedded JPEG from a seekable source.
pub fn decode_largest_embedded_jpeg_from_reader<R>(
    reader: &mut R,
) -> Result<Option<RgbaImage>, image::ImageError>
where
    R: Read + Seek,
{
    let Some(info) = embedded_preview_info_from_reader(reader)? else {
        return Ok(None);
    };
    let window = ReadWindow::new(reader, info.offset, info.length)?;
    ImageReader::with_format(BufReader::new(window), ImageFormat::Jpeg)
        .decode()
        .map(|image| Some(image.into_rgba8()))
}

fn jpeg_candidates(bytes: &[u8]) -> Vec<EmbeddedPreviewInfo> {
    const START: &[u8] = &[0xFF, 0xD8, 0xFF];
    const END: &[u8] = &[0xFF, 0xD9];

    let mut candidates = Vec::new();
    let mut search_from = 0;
    while search_from + START.len() <= bytes.len() {
        let Some(relative_start) = memmem::find(&bytes[search_from..], START) else {
            break;
        };
        let start = search_from + relative_start;
        let payload_start = start + START.len();
        let Some(relative_end) = memmem::find(&bytes[payload_start..], END) else {
            break;
        };
        let end = payload_start + relative_end + END.len();
        let candidate = &bytes[start..end];
        let dimensions =
            ImageReader::with_format(Cursor::new(candidate), ImageFormat::Jpeg).into_dimensions();
        if let Ok((width, height)) = dimensions {
            candidates.push(EmbeddedPreviewInfo {
                offset: start as u64,
                length: (end - start) as u64,
                width,
                height,
            });
        }
        search_from = payload_start;
    }
    candidates
}

fn scan_jpeg_ranges<R: Read>(reader: &mut R) -> io::Result<Vec<(u64, u64)>> {
    const BUFFER_SIZE: usize = 256 * 1024;
    const MAX_OPEN_STARTS: usize = 64;
    const MAX_CANDIDATES: usize = 256;
    const START: &[u8] = &[0xFF, 0xD8, 0xFF];
    const END: &[u8] = &[0xFF, 0xD9];

    let mut buffer = vec![0_u8; BUFFER_SIZE + 2];
    let mut total_read = 0_u64;
    let mut carry = 0_usize;
    let mut starts = Vec::new();
    let mut ranges = Vec::new();

    loop {
        let read = reader.read(&mut buffer[carry..])?;
        if read == 0 {
            break;
        }
        let available = carry + read;
        let base_offset = total_read.saturating_sub(carry as u64);
        let bytes = &buffer[..available];
        let mut events = memmem::find_iter(bytes, START)
            .filter_map(|relative| {
                let offset = base_offset + relative as u64;
                (offset + START.len() as u64 > total_read).then_some((offset, true))
            })
            .chain(memmem::find_iter(bytes, END).filter_map(|relative| {
                let offset = base_offset + relative as u64;
                (offset + END.len() as u64 > total_read).then_some((offset, false))
            }))
            .collect::<Vec<_>>();
        events.sort_unstable_by_key(|event| event.0);

        for (offset, is_start) in events {
            if is_start {
                if starts.len() == MAX_OPEN_STARTS {
                    starts.remove(0);
                }
                starts.push(offset);
            } else {
                for start in starts.drain(..) {
                    if ranges.len() == MAX_CANDIDATES {
                        return Ok(ranges);
                    }
                    ranges.push((start, offset + END.len() as u64 - start));
                }
            }
        }

        total_read += read as u64;
        carry = available.min(2);
        buffer.copy_within(available - carry..available, 0);
    }
    Ok(ranges)
}

struct ReadWindow<'a, R> {
    inner: &'a mut R,
    start: u64,
    length: u64,
    position: u64,
}

impl<'a, R: Seek> ReadWindow<'a, R> {
    fn new(inner: &'a mut R, start: u64, length: u64) -> io::Result<Self> {
        inner.seek(SeekFrom::Start(start))?;
        Ok(Self {
            inner,
            start,
            length,
            position: 0,
        })
    }
}

impl<R: Read> Read for ReadWindow<'_, R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let remaining = self.length.saturating_sub(self.position);
        let allowed = usize::try_from(remaining.min(buffer.len() as u64)).unwrap_or(buffer.len());
        let read = self.inner.read(&mut buffer[..allowed])?;
        self.position += read as u64;
        Ok(read)
    }
}

impl<R: Seek> Seek for ReadWindow<'_, R> {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let target = match position {
            SeekFrom::Start(offset) => i128::from(offset),
            SeekFrom::End(offset) => i128::from(self.length) + i128::from(offset),
            SeekFrom::Current(offset) => i128::from(self.position) + i128::from(offset),
        };
        if !(0..=i128::from(self.length)).contains(&target) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "seek outside embedded JPEG window",
            ));
        }
        self.position = target as u64;
        self.inner
            .seek(SeekFrom::Start(self.start + self.position))?;
        Ok(self.position)
    }
}

#[cfg(test)]
mod tests {
    use image::{DynamicImage, ImageBuffer, Rgb, codecs::jpeg::JpegEncoder};

    use super::*;

    #[test]
    fn largest_valid_embedded_jpeg_is_selected() {
        let small = encode_jpeg(32, 16);
        let large = encode_jpeg(128, 64);
        let mut container = b"IIRO\x08\0\0\0noise".to_vec();
        container.extend_from_slice(&small);
        container.extend_from_slice(b"maker-note-noise");
        container.extend_from_slice(&large);

        let info = embedded_preview_info(&container).expect("preview should be found");
        assert_eq!((info.width, info.height), (128, 64));
        let decoded = decode_largest_embedded_jpeg(&container)
            .expect("preview should decode")
            .expect("preview should exist");
        assert_eq!(decoded.dimensions(), (128, 64));
    }

    #[test]
    fn random_data_is_not_treated_as_a_preview() {
        assert_eq!(embedded_preview_info(b"IIRO\x08\0\0\0not a jpeg"), None);
    }

    #[test]
    fn seekable_scanner_matches_in_memory_scanner_across_buffer_boundaries() {
        let small = encode_jpeg(32, 16);
        let large = encode_jpeg(128, 64);
        let mut container = vec![0_u8; 256 * 1024 - 2];
        container.extend_from_slice(&small);
        container.extend_from_slice(b"separator");
        container.extend_from_slice(&large);

        let expected = embedded_preview_info(&container).expect("preview should be found");
        let mut reader = Cursor::new(container);
        let actual = embedded_preview_info_from_reader(&mut reader)
            .expect("scan should succeed")
            .expect("preview should be found");
        assert_eq!(actual, expected);

        let decoded = decode_largest_embedded_jpeg_from_reader(&mut reader)
            .expect("decode should succeed")
            .expect("preview should exist");
        assert_eq!(decoded.dimensions(), (128, 64));
    }

    #[test]
    fn orf_preview_descriptor_avoids_scanning_sensor_data() {
        let jpeg = encode_jpeg(128, 64);
        let descriptor_offset = 128_usize;
        let preview_offset = 256_usize;
        let raw_offset = 65_536_usize;
        let mut container = vec![0_u8; raw_offset + 1024];
        container[..4].copy_from_slice(b"IIRO");
        container[4..8].copy_from_slice(&8_u32.to_le_bytes());
        container[8..10].copy_from_slice(&1_u16.to_le_bytes());
        container[10..12].copy_from_slice(&0x0111_u16.to_le_bytes());
        container[12..14].copy_from_slice(&4_u16.to_le_bytes());
        container[14..18].copy_from_slice(&1_u32.to_le_bytes());
        container[18..22].copy_from_slice(&(raw_offset as u32).to_le_bytes());
        container[descriptor_offset..descriptor_offset + 4].copy_from_slice(b"0100");
        container[descriptor_offset + 4..descriptor_offset + 8]
            .copy_from_slice(&1_u32.to_le_bytes());
        container[descriptor_offset + 8..descriptor_offset + 12]
            .copy_from_slice(&(preview_offset as u32).to_le_bytes());
        container[descriptor_offset + 12..descriptor_offset + 16]
            .copy_from_slice(&(jpeg.len() as u32).to_le_bytes());
        container[preview_offset..preview_offset + jpeg.len()].copy_from_slice(&jpeg);

        let mut reader = Cursor::new(container);
        let info = embedded_preview_info_from_reader(&mut reader)
            .expect("descriptor should parse")
            .expect("preview should exist");
        assert_eq!(info.offset, preview_offset as u64);
        assert_eq!(info.length, jpeg.len() as u64);
        assert_eq!((info.width, info.height), (128, 64));
    }

    fn encode_jpeg(width: u32, height: u32) -> Vec<u8> {
        let image = ImageBuffer::from_pixel(width, height, Rgb([20_u8, 40, 80]));
        let mut encoded = Vec::new();
        JpegEncoder::new(&mut encoded)
            .encode_image(&DynamicImage::ImageRgb8(image))
            .expect("test JPEG should encode");
        encoded
    }
}
