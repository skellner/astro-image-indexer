use serde::{Deserialize, Serialize};

/// Parsed metadata from a FITS or XISF file.
/// All fields are optional — not every file will have every keyword.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImageMetadata {
    pub format: String,          // "FITS" or "XISF"
    pub image_type: Option<String>,
    pub object_name: Option<String>,
    pub ra: Option<f64>,
    pub dec: Option<f64>,
    pub date_obs: Option<String>,
    pub exposure_time: Option<f64>,
    pub gain: Option<f64>,
    pub offset: Option<i64>,
    pub iso: Option<i64>,
    pub filter_name: Option<String>,
    pub binning_x: Option<i64>,
    pub binning_y: Option<i64>,
    pub telescope: Option<String>,
    pub instrument: Option<String>,
    pub focal_length: Option<f64>,
    pub aperture: Option<f64>,
    pub ccd_temp: Option<f64>,
    pub site_lat: Option<f64>,
    pub site_lon: Option<f64>,
    pub airmass: Option<f64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub bit_depth: Option<i64>,
    pub software: Option<String>,
    // Sub-quality — populated externally, not from file headers
    pub fwhm: Option<f64>,
    pub eccentricity: Option<f64>,
    pub star_count: Option<i64>,
    pub snr: Option<f64>,
    pub sky_background: Option<f64>,
}
