use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use thiserror::Error;
use quick_xml::events::Event;
use quick_xml::Reader;

use crate::metadata::ImageMetadata;

#[derive(Debug, Error)]
pub enum XisfError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Not a valid XISF file")]
    NotXisf,
    #[error("XML parse error: {0}")]
    Xml(#[from] quick_xml::Error),
    #[error("Header length field is invalid")]
    BadHeaderLength,
}

/// XISF file signature: "XISF0100"
const MAGIC: &[u8; 8] = b"XISF0100";

/// Parse an XISF file and return structured metadata + raw property map.
pub fn parse(path: &Path) -> Result<(ImageMetadata, HashMap<String, String>), XisfError> {
    let xml = read_xml_header(path)?;
    let (fits_kws, properties, image_attrs) = parse_xml(&xml)?;

    // Merge FITS keywords and XISF properties into one raw map.
    // XISF properties take precedence (more structured).
    let mut raw: HashMap<String, String> = fits_kws;
    for (k, v) in &properties {
        raw.insert(k.clone(), v.clone());
    }

    let meta = extract_metadata(&raw, &properties, &image_attrs);
    Ok((meta, raw))
}

// ---------------------------------------------------------------------------
// File header → XML bytes
// ---------------------------------------------------------------------------

fn read_xml_header(path: &Path) -> Result<String, XisfError> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);

    // Signature: 8 bytes
    let mut magic = [0u8; 8];
    reader.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(XisfError::NotXisf);
    }

    // Header length: 4 bytes little-endian u32
    let mut len_bytes = [0u8; 4];
    reader.read_exact(&mut len_bytes)?;
    let header_len = u32::from_le_bytes(len_bytes) as usize;
    if header_len == 0 || header_len > 64 * 1024 * 1024 {
        return Err(XisfError::BadHeaderLength);
    }

    // Reserved: 4 bytes (skip)
    let mut reserved = [0u8; 4];
    reader.read_exact(&mut reserved)?;

    // XML header
    let mut xml_bytes = vec![0u8; header_len];
    reader.read_exact(&mut xml_bytes)?;

    String::from_utf8(xml_bytes).map_err(|_| XisfError::NotXisf)
}

// ---------------------------------------------------------------------------
// XML parsing
// ---------------------------------------------------------------------------

/// Returns (fits_keywords_map, xisf_properties_map, image_attributes_map)
fn parse_xml(
    xml: &str,
) -> Result<
    (
        HashMap<String, String>,
        HashMap<String, String>,
        HashMap<String, String>,
    ),
    XisfError,
> {
    let mut fits_kws: HashMap<String, String> = HashMap::new();
    let mut properties: HashMap<String, String> = HashMap::new();
    let mut image_attrs: HashMap<String, String> = HashMap::new();
    let mut inside_image = false;

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let name = std::str::from_utf8(e.name().as_ref())
                    .unwrap_or("")
                    .to_string();

                match name.as_str() {
                    "Image" => {
                        inside_image = true;
                        // Capture image-level attributes: geometry, sampleFormat, colorSpace
                        for attr in e.attributes().flatten() {
                            let key = std::str::from_utf8(attr.key.as_ref())
                                .unwrap_or("")
                                .to_string();
                            let val = attr
                                .decode_and_unescape_value(reader.decoder())
                                .unwrap_or_default()
                                .to_string();
                            image_attrs.insert(key, val);
                        }
                    }
                    "FITSKeyword" if inside_image => {
                        let mut kw_name = String::new();
                        let mut kw_value = String::new();
                        for attr in e.attributes().flatten() {
                            match std::str::from_utf8(attr.key.as_ref()).unwrap_or("") {
                                "name" => {
                                    kw_name = attr
                                        .decode_and_unescape_value(reader.decoder())
                                        .unwrap_or_default()
                                        .trim()
                                        .to_uppercase();
                                }
                                "value" => {
                                    kw_value = attr
                                        .decode_and_unescape_value(reader.decoder())
                                        .unwrap_or_default()
                                        .trim_matches('\'')
                                        .trim()
                                        .to_string();
                                }
                                _ => {}
                            }
                        }
                        if !kw_name.is_empty() {
                            fits_kws.insert(kw_name, kw_value);
                        }
                    }
                    "Property" if inside_image => {
                        let mut prop_id = String::new();
                        let mut prop_value = String::new();
                        for attr in e.attributes().flatten() {
                            match std::str::from_utf8(attr.key.as_ref()).unwrap_or("") {
                                "id" => {
                                    prop_id = attr
                                        .decode_and_unescape_value(reader.decoder())
                                        .unwrap_or_default()
                                        .to_string();
                                }
                                "value" => {
                                    prop_value = attr
                                        .decode_and_unescape_value(reader.decoder())
                                        .unwrap_or_default()
                                        .to_string();
                                }
                                _ => {}
                            }
                        }
                        if !prop_id.is_empty() {
                            properties.insert(prop_id, prop_value);
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                if std::str::from_utf8(e.name().as_ref()).unwrap_or("") == "Image" {
                    // Only parse the first Image element
                    break;
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(XisfError::Xml(e)),
            _ => {}
        }
    }

    Ok((fits_kws, properties, image_attrs))
}

// ---------------------------------------------------------------------------
// Extraction helpers
// ---------------------------------------------------------------------------

fn extract_metadata(
    raw: &HashMap<String, String>,
    props: &HashMap<String, String>,
    image_attrs: &HashMap<String, String>,
) -> ImageMetadata {
    let get_raw = |keys: &[&str]| -> Option<String> {
        keys.iter()
            .find_map(|k| raw.get(*k).filter(|v| !v.is_empty()).cloned())
    };
    let get_prop = |keys: &[&str]| -> Option<String> {
        keys.iter()
            .find_map(|k| props.get(*k).filter(|v| !v.is_empty()).cloned())
    };
    let get_f64 = |keys: &[&str]| -> Option<f64> {
        get_raw(keys).and_then(|v| v.parse().ok())
    };
    let get_i64 = |keys: &[&str]| -> Option<i64> {
        get_raw(keys).and_then(|v| v.parse().ok())
    };

    // Image dimensions from geometry attribute: "width:height:channels"
    let (width, height, bit_depth) = parse_geometry(image_attrs);

    // sampleFormat → bit depth if geometry didn't provide it
    let bit_depth = bit_depth.or_else(|| {
        image_attrs
            .get("sampleFormat")
            .and_then(|s| sample_format_bits(s))
    });

    // XISF-native properties override FITS keywords for these fields
    let object_name = get_prop(&["Observation:Object:Name"])
        .or_else(|| get_raw(&["OBJECT", "TARGET"]));

    let date_obs = get_prop(&["Observation:Time:Start"])
        .or_else(|| get_raw(&["DATE-OBS", "DATE_OBS", "DATE-BEG"]));

    let exposure_time = get_prop(&["Instrument:ExposureTime"])
        .and_then(|v| v.parse().ok())
        .or_else(|| get_f64(&["EXPTIME", "EXPOSURE"]));

    let gain = get_prop(&["Instrument:Camera:Gain"])
        .and_then(|v| v.parse().ok())
        .or_else(|| get_f64(&["GAIN"]));

    let iso = get_prop(&["Instrument:Camera:ISOSpeed"])
        .and_then(|v| v.parse().ok())
        .or_else(|| get_i64(&["ISOSPEED", "ISO"]));

    let filter_name = get_prop(&["Instrument:Filter:Name"])
        .or_else(|| get_raw(&["FILTER", "FILTNAM1"]));

    let telescope = get_prop(&["Instrument:Telescope:Name"])
        .or_else(|| get_raw(&["TELESCOP"]));

    let instrument = get_prop(&["Instrument:Camera:Name"])
        .or_else(|| get_raw(&["INSTRUME"]));

    let focal_length = get_prop(&["Instrument:Telescope:FocalLength"])
        .and_then(|v| v.parse().ok())
        .or_else(|| get_f64(&["FOCALLEN"]));

    let ccd_temp = get_prop(&["Instrument:Sensor:Temperature"])
        .and_then(|v| v.parse().ok())
        .or_else(|| get_f64(&["CCD-TEMP", "CCDTEMP", "TEMP"]));

    let site_lat = get_prop(&["Observation:Location:Latitude"])
        .and_then(|v| v.parse().ok())
        .or_else(|| get_f64(&["SITELAT", "LAT-OBS"]));

    let site_lon = get_prop(&["Observation:Location:Longitude"])
        .and_then(|v| v.parse().ok())
        .or_else(|| get_f64(&["SITELONG", "LONG-OBS"]));

    let software = get_prop(&["PCL:CreationTime"]) // PixInsight writes this
        .and(get_prop(&["Creator:Application"]))
        .or_else(|| get_raw(&["SWCREATE", "CREATOR", "PROGRAM"]));

    ImageMetadata {
        format: "XISF".into(),
        image_type: get_raw(&["IMAGETYP", "FRAME"]).map(|s| crate::metadata::normalize_image_type(&s)),
        object_name,
        ra: get_f64(&["RA", "OBJCTRA"]),
        dec: get_f64(&["DEC", "OBJCTDEC"]),
        date_obs,
        exposure_time,
        gain,
        offset: get_i64(&["OFFSET", "PEDESTAL"]),
        iso,
        filter_name,
        binning_x: get_i64(&["XBINNING"]),
        binning_y: get_i64(&["YBINNING"]),
        telescope,
        instrument,
        focal_length,
        aperture: get_f64(&["APERTURE"]),
        ccd_temp,
        site_lat,
        site_lon,
        airmass: get_f64(&["AIRMASS"]),
        width,
        height,
        bit_depth,
        software,
        fwhm: None,
        eccentricity: None,
        star_count: None,
        snr: None,
        sky_background: None,
    }
}

/// Parse geometry attribute "width:height:channels" → (width, height, None)
fn parse_geometry(
    attrs: &HashMap<String, String>,
) -> (Option<i64>, Option<i64>, Option<i64>) {
    let geom = match attrs.get("geometry") {
        Some(g) => g,
        None => return (None, None, None),
    };
    let parts: Vec<&str> = geom.split(':').collect();
    let w = parts.first().and_then(|s| s.parse().ok());
    let h = parts.get(1).and_then(|s| s.parse().ok());
    (w, h, None)
}

/// Map XISF sampleFormat string to bit depth integer
fn sample_format_bits(fmt: &str) -> Option<i64> {
    match fmt {
        "UInt8" => Some(8),
        "UInt16" => Some(16),
        "UInt32" => Some(32),
        "Float32" => Some(32),
        "Float64" => Some(64),
        "Complex32" => Some(32),
        "Complex64" => Some(64),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<xisf version="1.0" xmlns="http://www.pixinsight.com/xisf">
  <Image geometry="4656:3520:1" sampleFormat="UInt16" colorSpace="Gray">
    <FITSKeyword name="IMAGETYP" value="'Light Frame'" comment="Type of image" />
    <FITSKeyword name="EXPTIME" value="300.0" comment="Exposure time in seconds" />
    <FITSKeyword name="FILTER" value="'Ha'" comment="Filter name" />
    <FITSKeyword name="CCD-TEMP" value="-10.0" comment="CCD temperature" />
    <Property id="Observation:Object:Name" type="String" value="NGC 7000" />
    <Property id="Instrument:Camera:Gain" type="Float32" value="100" />
    <Property id="Observation:Location:Latitude" type="Float64" value="48.2" />
    <Property id="Observation:Location:Longitude" type="Float64" value="16.3" />
  </Image>
</xisf>"#;

    #[test]
    fn test_parse_xml_fits_keywords() {
        let (fits_kws, _, _) = parse_xml(SAMPLE_XML).unwrap();
        assert_eq!(fits_kws.get("EXPTIME").map(|s| s.as_str()), Some("300.0"));
        assert_eq!(fits_kws.get("FILTER").map(|s| s.as_str()), Some("Ha"));
        assert_eq!(fits_kws.get("CCD-TEMP").map(|s| s.as_str()), Some("-10.0"));
    }

    #[test]
    fn test_parse_xml_properties() {
        let (_, props, _) = parse_xml(SAMPLE_XML).unwrap();
        assert_eq!(
            props.get("Observation:Object:Name").map(|s| s.as_str()),
            Some("NGC 7000")
        );
        assert_eq!(
            props.get("Instrument:Camera:Gain").map(|s| s.as_str()),
            Some("100")
        );
    }

    #[test]
    fn test_parse_xml_image_attrs() {
        let (_, _, attrs) = parse_xml(SAMPLE_XML).unwrap();
        assert_eq!(attrs.get("geometry").map(|s| s.as_str()), Some("4656:3520:1"));
        assert_eq!(attrs.get("sampleFormat").map(|s| s.as_str()), Some("UInt16"));
    }

    #[test]
    fn test_extract_metadata_prefers_properties() {
        let (fits_kws, props, attrs) = parse_xml(SAMPLE_XML).unwrap();
        let mut raw = fits_kws;
        raw.extend(props.clone());
        let meta = extract_metadata(&raw, &props, &attrs);

        assert_eq!(meta.format, "XISF");
        assert_eq!(meta.object_name.as_deref(), Some("NGC 7000")); // from Property
        assert_eq!(meta.exposure_time, Some(300.0));               // from FITSKeyword
        assert_eq!(meta.gain, Some(100.0));                        // from Property
        assert_eq!(meta.filter_name.as_deref(), Some("Ha"));
        assert_eq!(meta.ccd_temp, Some(-10.0));
        assert_eq!(meta.width, Some(4656));
        assert_eq!(meta.height, Some(3520));
        assert_eq!(meta.bit_depth, Some(16));
        assert_eq!(meta.site_lat, Some(48.2));
        assert_eq!(meta.site_lon, Some(16.3));
    }

    #[test]
    fn test_sample_format_bits() {
        assert_eq!(sample_format_bits("UInt16"), Some(16));
        assert_eq!(sample_format_bits("Float32"), Some(32));
        assert_eq!(sample_format_bits("UInt8"), Some(8));
        assert_eq!(sample_format_bits("bogus"), None);
    }

    #[test]
    fn test_parse_geometry() {
        let mut attrs = HashMap::new();
        attrs.insert("geometry".to_string(), "4656:3520:3".to_string());
        let (w, h, _) = parse_geometry(&attrs);
        assert_eq!(w, Some(4656));
        assert_eq!(h, Some(3520));
    }
}
