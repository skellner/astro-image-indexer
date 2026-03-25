use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use thiserror::Error;

use crate::metadata::ImageMetadata;

#[derive(Debug, Error)]
pub enum FitsError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Not a valid FITS file")]
    NotFits,
    #[error("Header END card not found within limit")]
    NoEnd,
}

const BLOCK_SIZE: usize = 2880;
const CARD_SIZE: usize = 80;
const CARDS_PER_BLOCK: usize = BLOCK_SIZE / CARD_SIZE;
/// Max blocks to read before giving up (covers pathologically large headers)
const MAX_HEADER_BLOCKS: usize = 100;

/// Parse a FITS file and return structured metadata + raw header map.
pub fn parse(path: &Path) -> Result<(ImageMetadata, HashMap<String, String>), FitsError> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let cards = read_header_cards(&mut reader)?;
    let raw = cards_to_map(&cards);
    let meta = extract_metadata(&raw, path);
    Ok((meta, raw))
}

// ---------------------------------------------------------------------------
// Header reading
// ---------------------------------------------------------------------------

fn read_header_cards(reader: &mut impl Read) -> Result<Vec<[u8; CARD_SIZE]>, FitsError> {
    let mut block = [0u8; BLOCK_SIZE];
    let mut cards: Vec<[u8; CARD_SIZE]> = Vec::new();
    let mut found_end = false;

    // Verify FITS magic: first card must start with "SIMPLE  ="
    reader.read_exact(&mut block)?;
    if &block[0..8] != b"SIMPLE  " {
        return Err(FitsError::NotFits);
    }

    for offset in (0..BLOCK_SIZE).step_by(CARD_SIZE) {
        let mut card = [0u8; CARD_SIZE];
        card.copy_from_slice(&block[offset..offset + CARD_SIZE]);
        if card_keyword(&card) == "END" {
            found_end = true;
            break;
        }
        cards.push(card);
    }

    if found_end {
        return Ok(cards);
    }

    for _ in 1..MAX_HEADER_BLOCKS {
        reader.read_exact(&mut block).map_err(|_| FitsError::NoEnd)?;
        for i in 0..CARDS_PER_BLOCK {
            let offset = i * CARD_SIZE;
            let mut card = [0u8; CARD_SIZE];
            card.copy_from_slice(&block[offset..offset + CARD_SIZE]);
            if card_keyword(&card) == "END" {
                return Ok(cards);
            }
            cards.push(card);
        }
    }

    Err(FitsError::NoEnd)
}

fn card_keyword(card: &[u8; CARD_SIZE]) -> String {
    std::str::from_utf8(&card[0..8])
        .unwrap_or("")
        .trim()
        .to_uppercase()
}

// ---------------------------------------------------------------------------
// Card → key/value map
// ---------------------------------------------------------------------------

fn cards_to_map(cards: &[[u8; CARD_SIZE]]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for card in cards {
        let raw = std::str::from_utf8(card).unwrap_or("");
        let keyword = raw[0..8].trim().to_uppercase();
        if keyword.is_empty() || keyword == "COMMENT" || keyword == "HISTORY" {
            continue;
        }
        // Value indicator: bytes 8-9 must be "= " for a value card
        if raw.len() > 9 && &raw[8..10] == "= " {
            let value_comment = &raw[10..];
            let value = parse_value(value_comment);
            map.insert(keyword, value);
        }
    }
    map
}

/// Extract the value portion of a FITS card value field, stripping comments.
fn parse_value(s: &str) -> String {
    let s = s.trim();
    if s.starts_with('\'') {
        // String value: ends at next unescaped single quote
        let inner = &s[1..];
        let mut result = String::new();
        let mut chars = inner.chars();
        loop {
            match chars.next() {
                None => break,
                Some('\'') => {
                    // Escaped quote '' → literal '
                    if chars.as_str().starts_with('\'') {
                        chars.next();
                        result.push('\'');
                    } else {
                        break;
                    }
                }
                Some(c) => result.push(c),
            }
        }
        result.trim_end().to_string()
    } else {
        // Numeric or logical: take everything before '/'
        let value_part = s.split('/').next().unwrap_or(s);
        value_part.trim().to_string()
    }
}

// ---------------------------------------------------------------------------
// Map → ImageMetadata
// ---------------------------------------------------------------------------

fn extract_metadata(raw: &HashMap<String, String>, path: &Path) -> ImageMetadata {
    let get = |keys: &[&str]| -> Option<String> {
        keys.iter()
            .find_map(|k| raw.get(*k).filter(|v| !v.is_empty()).cloned())
    };

    let get_f64 = |keys: &[&str]| -> Option<f64> {
        get(keys).and_then(|v| v.parse().ok())
    };

    let get_i64 = |keys: &[&str]| -> Option<i64> {
        get(keys).and_then(|v| v.parse().ok())
    };

    // RA/Dec: FITS stores as "HH MM SS.ss" strings or decimal degrees depending on software
    let ra = get_f64(&["RA", "OBJCTRA"]).or_else(|| {
        get(&["OBJCTRA"]).and_then(|s| parse_ra_string(&s))
    });
    let dec = get_f64(&["DEC", "OBJCTDEC"]).or_else(|| {
        get(&["OBJCTDEC"]).and_then(|s| parse_dec_string(&s))
    });

    ImageMetadata {
        format: "FITS".into(),
        image_type: get(&["IMAGETYP", "FRAME"]),
        object_name: get(&["OBJECT", "TARGET"]),
        ra,
        dec,
        date_obs: get(&["DATE-OBS", "DATE_OBS", "DATE-BEG"]),
        exposure_time: get_f64(&["EXPTIME", "EXPOSURE"]),
        gain: get_f64(&["GAIN"]),
        offset: get_i64(&["OFFSET", "PEDESTAL"]),
        iso: get_i64(&["ISOSPEED", "ISO"]),
        filter_name: get(&["FILTER", "FILTNAM1"]),
        binning_x: get_i64(&["XBINNING"]),
        binning_y: get_i64(&["YBINNING"]),
        telescope: get(&["TELESCOP"]),
        instrument: get(&["INSTRUME"]),
        focal_length: get_f64(&["FOCALLEN"]),
        aperture: get_f64(&["APERTURE"]),
        ccd_temp: get_f64(&["CCD-TEMP", "CCDTEMP", "TEMP"]),
        site_lat: get_f64(&["SITELAT", "LAT-OBS", "OBSLAT"]),
        site_lon: get_f64(&["SITELONG", "LONG-OBS", "OBSLONG"]),
        airmass: get_f64(&["AIRMASS"]),
        width: get_i64(&["NAXIS1"]),
        height: get_i64(&["NAXIS2"]),
        bit_depth: get_i64(&["BITPIX"]),
        software: get(&["SWCREATE", "CREATOR", "PROGRAM"]),
        // Sub-quality fields not present in raw FITS headers; populated later
        fwhm: None,
        eccentricity: None,
        star_count: None,
        snr: None,
        sky_background: None,
    }
}

// ---------------------------------------------------------------------------
// RA/Dec string helpers (sexagesimal → decimal degrees)
// ---------------------------------------------------------------------------

/// "HH MM SS.sss" → decimal degrees (×15)
fn parse_ra_string(s: &str) -> Option<f64> {
    let parts: Vec<f64> = s.split_whitespace()
        .filter_map(|p| p.parse().ok())
        .collect();
    match parts.as_slice() {
        [h, m, sec] => Some((h + m / 60.0 + sec / 3600.0) * 15.0),
        [h, m] => Some((h + m / 60.0) * 15.0),
        _ => None,
    }
}

/// "+DD MM SS.sss" → decimal degrees
fn parse_dec_string(s: &str) -> Option<f64> {
    let s = s.trim();
    let negative = s.starts_with('-');
    let s = s.trim_start_matches(['+', '-']);
    let parts: Vec<f64> = s.split_whitespace()
        .filter_map(|p| p.parse().ok())
        .collect();
    let deg = match parts.as_slice() {
        [d, m, sec] => d + m / 60.0 + sec / 3600.0,
        [d, m] => d + m / 60.0,
        _ => return None,
    };
    Some(if negative { -deg } else { deg })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_card(key: &str, value: &str) -> [u8; 80] {
        let record = format!("{:<8}= {:<70}", key, value);
        let mut card = [b' '; 80];
        let bytes = record.as_bytes();
        card[..bytes.len().min(80)].copy_from_slice(&bytes[..bytes.len().min(80)]);
        card
    }

    fn make_end_card() -> [u8; 80] {
        let mut card = [b' '; 80];
        card[..3].copy_from_slice(b"END");
        card
    }

    #[test]
    fn test_parse_string_value() {
        assert_eq!(parse_value("'NGC 1234   '"), "NGC 1234");
        assert_eq!(parse_value("'it''s'"), "it's");
    }

    #[test]
    fn test_parse_numeric_value() {
        assert_eq!(parse_value("120.5 / exposure time"), "120.5");
        assert_eq!(parse_value("T"), "T");
    }

    #[test]
    fn test_ra_dec_parse() {
        let ra = parse_ra_string("05 34 32.0").unwrap();
        assert!((ra - 83.633).abs() < 0.01);

        let dec = parse_dec_string("+22 00 52.0").unwrap();
        assert!((dec - 22.014).abs() < 0.01);

        let dec_neg = parse_dec_string("-05 23 28.0").unwrap();
        assert!((dec_neg - (-5.391)).abs() < 0.01);
    }

    #[test]
    fn test_cards_to_map() {
        let cards = vec![
            make_card("OBJECT  ", "'M42     '"),
            make_card("EXPTIME ", "300.0 / seconds"),
            make_card("IMAGETYP", "'Light Frame'"),
            make_end_card(),
        ];
        let map = cards_to_map(&cards);
        assert_eq!(map.get("OBJECT").map(|s| s.as_str()), Some("M42"));
        assert_eq!(map.get("EXPTIME").map(|s| s.as_str()), Some("300.0"));
        assert_eq!(map.get("IMAGETYP").map(|s| s.as_str()), Some("Light Frame"));
    }
}
