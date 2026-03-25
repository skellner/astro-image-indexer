use std::fs::File;
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::Path;

use base64::Engine;
use image::{GrayImage, Luma};
use quick_xml::events::Event;
use quick_xml::Reader;

// ---------------------------------------------------------------------------
// Public command
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_image_preview(file_path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Path::new(&file_path);
        let ext = p
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        match ext.as_str() {
            "fits" | "fit" => load_fits_preview(p),
            "xisf" => load_xisf_preview(p),
            _ => Err(format!("Unsupported format: {ext}")),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---------------------------------------------------------------------------
// FITS
// ---------------------------------------------------------------------------

fn load_fits_preview(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;

    let mut bitpix: i32 = 16;
    let mut naxis1: usize = 0;
    let mut naxis2: usize = 0;
    let mut bzero: f64 = 0.0;
    let mut bscale: f64 = 1.0;
    let mut header_blocks: u64 = 0;
    let mut found_end = false;

    let mut buf = [0u8; 2880];
    while !found_end {
        file.read_exact(&mut buf).map_err(|e| e.to_string())?;
        header_blocks += 1;
        for card in buf.chunks(80) {
            let s = std::str::from_utf8(card).unwrap_or("").trim_end();
            if s.starts_with("END") {
                found_end = true;
                break;
            }
            if let Some(v) = fits_card_value(s, "BITPIX") {
                bitpix = v.trim().parse().unwrap_or(16);
            } else if let Some(v) = fits_card_value(s, "NAXIS1") {
                naxis1 = v.trim().parse().unwrap_or(0);
            } else if let Some(v) = fits_card_value(s, "NAXIS2") {
                naxis2 = v.trim().parse().unwrap_or(0);
            } else if let Some(v) = fits_card_value(s, "BZERO") {
                bzero = v.trim().parse().unwrap_or(0.0);
            } else if let Some(v) = fits_card_value(s, "BSCALE") {
                bscale = v.trim().parse().unwrap_or(1.0);
            }
        }
    }

    if naxis1 == 0 || naxis2 == 0 {
        return Err("Could not determine image dimensions".into());
    }

    file.seek(SeekFrom::Start(header_blocks * 2880))
        .map_err(|e| e.to_string())?;

    let n_pixels = naxis1 * naxis2;
    let pixels = read_fits_pixels(&mut file, bitpix, n_pixels, bzero, bscale)?;
    stretch_and_encode(&pixels, naxis1, naxis2)
}

fn fits_card_value<'a>(card: &'a str, key: &str) -> Option<&'a str> {
    if !card.starts_with(key) {
        return None;
    }
    let rest = card[key.len()..].trim_start();
    if !rest.starts_with('=') {
        return None;
    }
    let after_eq = rest[1..].trim_start();
    // Drop inline comment after '/'
    Some(after_eq.split('/').next().unwrap_or("").trim_end())
}

fn read_fits_pixels(
    file: &mut File,
    bitpix: i32,
    n_pixels: usize,
    bzero: f64,
    bscale: f64,
) -> Result<Vec<f32>, String> {
    let mut pixels = vec![0f32; n_pixels];
    match bitpix {
        8 => {
            let mut raw = vec![0u8; n_pixels];
            file.read_exact(&mut raw).map_err(|e| e.to_string())?;
            for (i, &v) in raw.iter().enumerate() {
                pixels[i] = (bzero + bscale * v as f64) as f32;
            }
        }
        16 => {
            let mut raw = vec![0u8; n_pixels * 2];
            file.read_exact(&mut raw).map_err(|e| e.to_string())?;
            for (i, chunk) in raw.chunks_exact(2).enumerate() {
                let v = i16::from_be_bytes([chunk[0], chunk[1]]) as f64;
                pixels[i] = (bzero + bscale * v) as f32;
            }
        }
        32 => {
            let mut raw = vec![0u8; n_pixels * 4];
            file.read_exact(&mut raw).map_err(|e| e.to_string())?;
            for (i, chunk) in raw.chunks_exact(4).enumerate() {
                let v = i32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]) as f64;
                pixels[i] = (bzero + bscale * v) as f32;
            }
        }
        -32 => {
            let mut raw = vec![0u8; n_pixels * 4];
            file.read_exact(&mut raw).map_err(|e| e.to_string())?;
            for (i, chunk) in raw.chunks_exact(4).enumerate() {
                pixels[i] = f32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            }
        }
        -64 => {
            let mut raw = vec![0u8; n_pixels * 8];
            file.read_exact(&mut raw).map_err(|e| e.to_string())?;
            for (i, chunk) in raw.chunks_exact(8).enumerate() {
                pixels[i] = f64::from_be_bytes([
                    chunk[0], chunk[1], chunk[2], chunk[3],
                    chunk[4], chunk[5], chunk[6], chunk[7],
                ]) as f32;
            }
        }
        _ => return Err(format!("Unsupported BITPIX: {bitpix}")),
    }
    Ok(pixels)
}

// ---------------------------------------------------------------------------
// XISF
// ---------------------------------------------------------------------------

fn load_xisf_preview(path: &Path) -> Result<String, String> {
    const MAGIC: &[u8; 8] = b"XISF0100";

    let mut file = File::open(path).map_err(|e| e.to_string())?;

    let mut magic = [0u8; 8];
    file.read_exact(&mut magic).map_err(|e| e.to_string())?;
    if &magic != MAGIC {
        return Err("Not a valid XISF file".into());
    }

    let mut len_bytes = [0u8; 4];
    file.read_exact(&mut len_bytes).map_err(|e| e.to_string())?;
    let header_len = u32::from_le_bytes(len_bytes) as usize;

    // Skip 4-byte reserved field
    let mut reserved = [0u8; 4];
    file.read_exact(&mut reserved).map_err(|e| e.to_string())?;

    let mut xml_bytes = vec![0u8; header_len];
    file.read_exact(&mut xml_bytes).map_err(|e| e.to_string())?;
    let xml = String::from_utf8(xml_bytes).map_err(|_| "Invalid UTF-8 in XISF header")?;

    // compression: Option<(codec, uncompressed_bytes, item_bytes)>
    let (width, height, fmt, data_offset, data_size, compression) =
        parse_xisf_image_info(&xml)?;
    if width == 0 || height == 0 {
        return Err("Could not determine XISF image dimensions".into());
    }

    file.seek(SeekFrom::Start(data_offset))
        .map_err(|e| e.to_string())?;

    let n_pixels = width * height;
    let read_bytes = data_size.unwrap_or(n_pixels * fmt.bytes_per_sample());
    let mut compressed_or_raw = vec![0u8; read_bytes];
    file.read_exact(&mut compressed_or_raw)
        .map_err(|e| e.to_string())?;

    // Decompress if needed (N.I.N.A. uses lz4+sh by default)
    let raw = match compression {
        Some((ref codec, uncompressed_bytes, item_size)) => {
            let decompressed = if codec.starts_with("lz4") {
                lz4_flex::decompress(&compressed_or_raw, uncompressed_bytes)
                    .map_err(|e| format!("LZ4 decompress failed: {e}"))?
            } else {
                return Err(format!("Unsupported XISF compression: {codec}"));
            };
            // "+sh" suffix means byte-shuffle was applied before compression;
            // undo the shuffle after decompressing.
            if codec.contains("+sh") {
                byte_unshuffle(&decompressed, item_size)
            } else {
                decompressed
            }
        }
        None => compressed_or_raw,
    };

    // For multi-channel images take only the first channel (first width*height samples)
    let pixels = fmt.decode(&raw, n_pixels)?;
    stretch_and_encode(&pixels, width, height)
}

/// Undo XISF byte-shuffle: input is [byte0 of all items, byte1 of all items, …],
/// output is [byte0, byte1, … of item0, byte0, byte1, … of item1, …].
fn byte_unshuffle(shuffled: &[u8], item_size: usize) -> Vec<u8> {
    if item_size <= 1 {
        return shuffled.to_vec();
    }
    let n_items = shuffled.len() / item_size;
    let mut out = vec![0u8; shuffled.len()];
    for i in 0..n_items {
        for j in 0..item_size {
            out[i * item_size + j] = shuffled[j * n_items + i];
        }
    }
    out
}

#[derive(Debug)]
enum SampleFormat {
    UInt8,
    UInt16,
    UInt32,
    Float32,
    Float64,
}

impl SampleFormat {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "UInt8" => Some(Self::UInt8),
            "UInt16" => Some(Self::UInt16),
            "UInt32" => Some(Self::UInt32),
            "Float32" => Some(Self::Float32),
            "Float64" => Some(Self::Float64),
            _ => None,
        }
    }

    fn bytes_per_sample(&self) -> usize {
        match self {
            Self::UInt8 => 1,
            Self::UInt16 => 2,
            Self::UInt32 => 4,
            Self::Float32 => 4,
            Self::Float64 => 8,
        }
    }

    fn decode(&self, raw: &[u8], n_pixels: usize) -> Result<Vec<f32>, String> {
        let mut pixels = vec![0f32; n_pixels];
        match self {
            Self::UInt8 => {
                for (i, &v) in raw.iter().take(n_pixels).enumerate() {
                    pixels[i] = v as f32 / 255.0;
                }
            }
            Self::UInt16 => {
                for (i, chunk) in raw.chunks_exact(2).take(n_pixels).enumerate() {
                    let v = u16::from_le_bytes([chunk[0], chunk[1]]);
                    pixels[i] = v as f32 / 65535.0;
                }
            }
            Self::UInt32 => {
                for (i, chunk) in raw.chunks_exact(4).take(n_pixels).enumerate() {
                    let v = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                    pixels[i] = v as f32 / u32::MAX as f32;
                }
            }
            Self::Float32 => {
                for (i, chunk) in raw.chunks_exact(4).take(n_pixels).enumerate() {
                    pixels[i] = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                }
            }
            Self::Float64 => {
                for (i, chunk) in raw.chunks_exact(8).take(n_pixels).enumerate() {
                    pixels[i] = f64::from_le_bytes([
                        chunk[0], chunk[1], chunk[2], chunk[3],
                        chunk[4], chunk[5], chunk[6], chunk[7],
                    ]) as f32;
                }
            }
        }
        Ok(pixels)
    }
}

// Returns (width, height, format, data_offset, data_size, compression)
// compression = Some((codec_name, uncompressed_bytes, item_bytes))
fn parse_xisf_image_info(
    xml: &str,
) -> Result<
    (usize, usize, SampleFormat, u64, Option<usize>, Option<(String, usize, usize)>),
    String,
> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let raw_name = e.name();
                let name = std::str::from_utf8(raw_name.as_ref()).unwrap_or("");
                if name != "Image" {
                    continue;
                }
                let mut geometry = String::new();
                let mut sample_format_str = "UInt16".to_string();
                let mut location = String::new();
                let mut compression_str = String::new();

                for attr in e.attributes().flatten() {
                    match std::str::from_utf8(attr.key.as_ref()).unwrap_or("") {
                        "geometry" => {
                            geometry = attr
                                .decode_and_unescape_value(reader.decoder())
                                .unwrap_or_default()
                                .to_string();
                        }
                        "sampleFormat" => {
                            sample_format_str = attr
                                .decode_and_unescape_value(reader.decoder())
                                .unwrap_or_default()
                                .to_string();
                        }
                        "location" => {
                            location = attr
                                .decode_and_unescape_value(reader.decoder())
                                .unwrap_or_default()
                                .to_string();
                        }
                        "compression" => {
                            compression_str = attr
                                .decode_and_unescape_value(reader.decoder())
                                .unwrap_or_default()
                                .to_string();
                        }
                        _ => {}
                    }
                }

                let parts: Vec<&str> = geometry.split(':').collect();
                let width: usize = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
                let height: usize = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
                let fmt = SampleFormat::parse(&sample_format_str).unwrap_or(SampleFormat::UInt16);

                // location format: "attachment:byte_offset:byte_size"
                let loc_parts: Vec<&str> = location.split(':').collect();
                if loc_parts.first().copied() != Some("attachment") {
                    return Err(format!("Unsupported XISF data location: {location}"));
                }
                let offset: u64 = loc_parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
                let size: Option<usize> = loc_parts.get(2).and_then(|s| s.parse().ok());

                // compression format: "codec:uncompressedBytes" or "codec:uncompressedBytes:itemBytes"
                // e.g. "lz4+sh:51924864:2"
                let compression = if compression_str.is_empty() {
                    None
                } else {
                    let cp: Vec<&str> = compression_str.split(':').collect();
                    let codec = cp.first().copied().unwrap_or("").to_string();
                    let uncompressed: usize =
                        cp.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
                    let item_size: usize = cp
                        .get(2)
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(fmt.bytes_per_sample());
                    Some((codec, uncompressed, item_size))
                };

                return Ok((width, height, fmt, offset, size, compression));
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(e.to_string()),
            _ => {}
        }
    }
    Err("No Image element found in XISF header".into())
}

// ---------------------------------------------------------------------------
// Stretch + PNG encode
// ---------------------------------------------------------------------------

fn stretch_and_encode(pixels: &[f32], width: usize, height: usize) -> Result<String, String> {
    if pixels.is_empty() || width == 0 || height == 0 {
        return Err("Empty image".into());
    }

    // Sample up to 100K pixels for percentile calculation
    let step = (pixels.len() / 100_000).max(1);
    let mut sampled: Vec<f32> = pixels
        .iter()
        .step_by(step)
        .copied()
        .filter(|v| v.is_finite())
        .collect();

    if sampled.is_empty() {
        return Err("No valid pixel values".into());
    }
    sampled.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    // Median-based stretch: works for light frames where stars are <1% of pixels.
    // A plain percentile clip would set `hi` inside the background noise because
    // the 99.5th percentile never reaches the star level.
    let median = sampled[sampled.len() / 2];
    let mut mad_vals: Vec<f32> = sampled.iter().map(|v| (v - median).abs()).collect();
    mad_vals.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let sigma = mad_vals[mad_vals.len() / 2] * 1.4826_f32;

    // Black point 3σ below sky; white point 50σ above sky.
    // Fall back to a percentile clip when sigma≈0 (pure flat / bias).
    let (lo, hi) = if sigma > 0.0 {
        let lo = (median - 3.0 * sigma).max(*sampled.first().unwrap_or(&0.0));
        let hi = median + 50.0 * sigma;
        (lo, hi)
    } else {
        let lo_idx = (sampled.len() as f64 * 0.001) as usize;
        let hi_idx = ((sampled.len() as f64 * 0.999) as usize).min(sampled.len() - 1);
        (sampled[lo_idx], sampled[hi_idx])
    };
    let range = hi - lo;
    if range <= 0.0 {
        return Err("Image has zero dynamic range".into());
    }

    // Downsample so the preview fits within 800×600
    const MAX_W: usize = 800;
    const MAX_H: usize = 600;
    let (out_w, out_h) = if width > MAX_W || height > MAX_H {
        let scale = (MAX_W as f64 / width as f64).min(MAX_H as f64 / height as f64);
        (
            (width as f64 * scale).round() as usize,
            (height as f64 * scale).round() as usize,
        )
    } else {
        (width, height)
    };

    let mut img = GrayImage::new(out_w as u32, out_h as u32);
    for y in 0..out_h {
        for x in 0..out_w {
            let src_x = (x * width / out_w).min(width - 1);
            let src_y = (y * height / out_h).min(height - 1);
            let val = pixels[src_y * width + src_x];
            // Square-root stretch: compresses background noise, opens faint signal
            let normalized = ((val - lo) / range).clamp(0.0, 1.0);
            img.put_pixel(x as u32, y as u32, Luma([(normalized.sqrt() * 255.0) as u8]));
        }
    }

    let mut png_buf = Vec::new();
    img.write_to(&mut Cursor::new(&mut png_buf), image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;

    let b64 = base64::engine::general_purpose::STANDARD.encode(&png_buf);
    Ok(format!("data:image/png;base64,{b64}"))
}
