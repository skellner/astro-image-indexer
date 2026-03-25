export interface ImageRow {
  id: number;
  file_name: string;
  file_path: string;
  format: string;
  image_type: string | null;
  object_name: string | null;
  filter_name: string | null;
  exposure_time: number | null;
  gain: number | null;
  date_obs: string | null;
  instrument: string | null;
  telescope: string | null;
  width: number | null;
  height: number | null;
  ccd_temp: number | null;
  software: string | null;
  fwhm: number | null;
  eccentricity: number | null;
  star_count: number | null;
  snr: number | null;
  quality_rejected: boolean;
  indexed_at: string;
  parse_error: string | null;
}

export interface ImageDetail extends ImageRow {
  ra: number | null;
  dec: number | null;
  iso: number | null;
  offset: number | null;
  binning_x: number | null;
  binning_y: number | null;
  focal_length: number | null;
  aperture: number | null;
  site_lat: number | null;
  site_lon: number | null;
  airmass: number | null;
  bit_depth: number | null;
  file_size: number | null;
  file_hash: string | null;
  sky_background: number | null;
  raw_headers: [string, string][];
}

export interface DirectoryEntry {
  id: number;
  path: string;
  added_at: string;
  last_scanned_at: string | null;
  image_count: number;
}

export interface LibraryStats {
  total_images: number;
  light_frames: number;
  total_exposure_hours: number;
  unique_objects: number;
  unique_filters: number;
}

export interface ScanProgress {
  current: number;
  total: number;
  file_name: string;
}

export interface ScanResult {
  indexed: number;
  skipped: number;
  errors: number;
  error_details: string[];
}
