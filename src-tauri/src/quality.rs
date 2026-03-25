use crate::preview::PixelBuffer;

// Limit star-search to a center crop of this size (pixels per side).
// Keeps analysis fast on large sensors while still sampling hundreds of stars.
const MAX_CROP: usize = 2048;

// Minimum margin from the crop edge required for a star candidate,
// so the FWHM walk always has room before hitting a boundary.
const MARGIN: usize = 12;

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Measure star count and median FWHM (in pixels) from a pixel buffer.
/// Returns None if no valid stars are found (e.g. the image is not a light frame
/// or has too few stars to measure).
pub fn analyse_stars(buf: &PixelBuffer) -> Option<(f64, i64)> {
    let (crop, cw, ch) = extract_crop(buf);
    let (background, sigma) = estimate_background(&crop);

    if sigma <= 0.0 {
        return None;
    }

    // 10-sigma threshold — conservative enough to catch faint stars, firm
    // enough to exclude hot pixels and cosmic rays.
    let threshold = background + 10.0 * sigma;

    let peaks = find_local_maxima(&crop, cw, ch, threshold);
    if peaks.is_empty() {
        return None;
    }

    let mut fwhm_values: Vec<f32> = peaks
        .iter()
        .filter_map(|&(x, y)| measure_fwhm(&crop, cw, ch, x, y, background))
        .filter(|&f| f >= 1.5 && f <= 25.0)
        .collect();

    if fwhm_values.is_empty() {
        return None;
    }

    fwhm_values.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median_fwhm = fwhm_values[fwhm_values.len() / 2] as f64;
    let star_count = fwhm_values.len() as i64;

    Some((median_fwhm, star_count))
}

// ---------------------------------------------------------------------------
// Center crop
// ---------------------------------------------------------------------------

fn extract_crop(buf: &PixelBuffer) -> (Vec<f32>, usize, usize) {
    let cw = buf.width.min(MAX_CROP);
    let ch = buf.height.min(MAX_CROP);
    let x0 = (buf.width - cw) / 2;
    let y0 = (buf.height - ch) / 2;
    let mut crop = Vec::with_capacity(cw * ch);
    for row in y0..y0 + ch {
        let start = row * buf.width + x0;
        crop.extend_from_slice(&buf.pixels[start..start + cw]);
    }
    (crop, cw, ch)
}

// ---------------------------------------------------------------------------
// Background estimation (median + MAD-based sigma)
// ---------------------------------------------------------------------------

fn estimate_background(pixels: &[f32]) -> (f32, f32) {
    let step = (pixels.len() / 50_000).max(1);
    let mut sample: Vec<f32> = pixels
        .iter()
        .step_by(step)
        .copied()
        .filter(|v| v.is_finite())
        .collect();

    if sample.is_empty() {
        return (0.0, 0.0);
    }
    sample.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let median = sample[sample.len() / 2];
    let mut mad: Vec<f32> = sample.iter().map(|v| (v - median).abs()).collect();
    mad.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let sigma = mad[mad.len() / 2] * 1.4826_f32;
    (median, sigma)
}

// ---------------------------------------------------------------------------
// Local maxima detection
// ---------------------------------------------------------------------------

fn find_local_maxima(
    pixels: &[f32],
    width: usize,
    height: usize,
    threshold: f32,
) -> Vec<(usize, usize)> {
    let mut peaks = Vec::new();
    for y in MARGIN..height.saturating_sub(MARGIN) {
        for x in MARGIN..width.saturating_sub(MARGIN) {
            let v = pixels[y * width + x];
            if v < threshold {
                continue;
            }
            // Must be strictly greater than all 24 neighbors in the 5×5 window
            let is_max = (-2i32..=2).all(|dy| {
                (-2i32..=2).all(|dx| {
                    if dx == 0 && dy == 0 {
                        return true;
                    }
                    let nx = (x as i32 + dx) as usize;
                    let ny = (y as i32 + dy) as usize;
                    pixels[ny * width + nx] < v
                })
            });
            if is_max {
                peaks.push((x, y));
            }
        }
    }
    peaks
}

// ---------------------------------------------------------------------------
// FWHM measurement for a single star
// ---------------------------------------------------------------------------

fn measure_fwhm(
    pixels: &[f32],
    width: usize,
    height: usize,
    cx: usize,
    cy: usize,
    background: f32,
) -> Option<f32> {
    let v_peak = pixels[cy * width + cx];
    let v_half = background + (v_peak - background) * 0.5;
    if v_half <= background {
        return None;
    }

    let walk = |dx: i32, dy: i32| -> f32 {
        let mut prev = v_peak;
        for d in 1usize..=30 {
            let nx = (cx as i32 + dx * d as i32) as usize;
            let ny = (cy as i32 + dy * d as i32) as usize;
            // Bounds check — should never trigger given MARGIN, but be safe
            if nx >= width || ny >= height {
                return d as f32;
            }
            let curr = pixels[ny * width + nx];
            if curr < v_half {
                // Linear interpolation for sub-pixel accuracy
                let frac = (prev - v_half) / (prev - curr).max(f32::EPSILON);
                return (d - 1) as f32 + frac;
            }
            prev = curr;
        }
        30.0
    };

    let fwhm_h = walk(1, 0) + walk(-1, 0);
    let fwhm_v = walk(0, 1) + walk(0, -1);
    Some((fwhm_h + fwhm_v) * 0.5)
}
