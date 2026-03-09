//! Calibration file download and caching.
//!
//! Downloads per-device calibration data from Saleae's servers and caches
//! it locally. The calibration URL is:
//!
//! ```text
//! https://downloads.saleae.com/calibration/{device_id_hex}.cal
//! ```
//!
//! Where `device_id_hex` is the 64-bit device ID in lowercase hexadecimal.

use std::fs;
use std::path::{Path, PathBuf};

use saleae_proto::calibration::{self, CalibrationData};

use crate::error::{DriverError, DriverResult};

/// Default directory for cached calibration files, relative to the
/// current working directory.
const CACHE_DIR: &str = "calibration";

/// Base URL for Saleae calibration file downloads.
const CALIBRATION_BASE_URL: &str = "https://downloads.saleae.com/calibration";

/// Download and parse calibration data for a device.
///
/// Checks the local cache first. If not found, downloads from Saleae's
/// servers and caches the raw XML for future use.
///
/// The cache directory is `calibration/` relative to `cache_root`. If
/// `cache_root` is `None`, uses the current working directory.
pub fn download_calibration(
    device_id: u64,
    cache_root: Option<&Path>,
) -> DriverResult<CalibrationData> {
    let hex_id = format!("{:x}", device_id);
    let filename = format!("{}.cal", hex_id);

    let cache_dir = match cache_root {
        Some(root) => root.join(CACHE_DIR),
        None => PathBuf::from(CACHE_DIR),
    };

    let cache_path = cache_dir.join(&filename);

    // Try to load from cache first.
    if cache_path.exists() {
        tracing::debug!(path = %cache_path.display(), "loading calibration from cache");
        let data = fs::read(&cache_path)?;
        return calibration::parse_calibration(&data).map_err(|e| {
            DriverError::CalibrationDownload(format!(
                "cached file {} is corrupt: {}",
                cache_path.display(),
                e
            ))
        });
    }

    // Download from Saleae's servers.
    let url = format!("{}/{}.cal", CALIBRATION_BASE_URL, hex_id);
    tracing::info!(%url, "downloading calibration file");

    let response = ureq::get(&url)
        .call()
        .map_err(|e| DriverError::CalibrationDownload(format!("HTTP request failed: {}", e)))?;

    let status = response.status();
    if status != 200 {
        return Err(DriverError::CalibrationDownload(format!(
            "HTTP {} for {}",
            status, url
        )));
    }

    let body = response
        .into_body()
        .read_to_vec()
        .map_err(|e| DriverError::CalibrationDownload(format!("failed to read body: {}", e)))?;

    if body.is_empty() {
        return Err(DriverError::CalibrationDownload(format!(
            "empty response from {}",
            url
        )));
    }

    // Parse before caching to ensure the data is valid.
    let cal_data = calibration::parse_calibration(&body).map_err(|e| {
        DriverError::CalibrationDownload(format!("downloaded file is corrupt: {}", e))
    })?;

    // Cache the raw XML for future use.
    if let Err(e) = fs::create_dir_all(&cache_dir) {
        tracing::warn!(
            error = %e,
            path = %cache_dir.display(),
            "failed to create calibration cache directory"
        );
    } else if let Err(e) = fs::write(&cache_path, &body) {
        tracing::warn!(
            error = %e,
            path = %cache_path.display(),
            "failed to write calibration cache file"
        );
    } else {
        tracing::debug!(path = %cache_path.display(), "cached calibration file");
    }

    Ok(cal_data)
}

/// Load calibration data from a local file (no download).
///
/// Useful for testing or when the file is already available locally.
pub fn load_calibration_file(path: &Path) -> DriverResult<CalibrationData> {
    let data = fs::read(path)?;
    calibration::parse_calibration(&data).map_err(|e| {
        DriverError::CalibrationDownload(format!("failed to parse {}: {}", path.display(), e))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn reference_cal_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("re")
            .join("calibration-764051b51bd4d9f1.cal")
    }

    #[test]
    fn load_reference_file() {
        let cal = load_calibration_file(&reference_cal_path()).expect("load reference cal");
        assert_eq!(cal.device_id, 8520900333282843121);
        assert_eq!(cal.device_type, 6);
        assert_eq!(cal.calibration_sets.len(), 48);
        assert_eq!(cal.voltage_ranges.len(), 8);
    }

    #[test]
    fn load_nonexistent_file() {
        let result = load_calibration_file(Path::new("/nonexistent/path.cal"));
        assert!(result.is_err());
    }

    #[test]
    fn cache_hit_skips_download() {
        // Set up a temp directory with a pre-cached file
        let tmp = std::env::temp_dir().join("saleae_cal_test_cache_hit");
        let cache_dir = tmp.join(CACHE_DIR);
        let _ = fs::create_dir_all(&cache_dir);

        // Copy reference file into cache with the expected name
        let src = reference_cal_path();
        let hex_id = format!("{:x}", 8520900333282843121u64);
        let dest = cache_dir.join(format!("{}.cal", hex_id));
        fs::copy(&src, &dest).expect("copy reference cal to cache");

        // Now download_calibration should use the cache
        let cal =
            download_calibration(8520900333282843121, Some(&tmp)).expect("download (cache hit)");
        assert_eq!(cal.device_id, 8520900333282843121);

        // Clean up
        let _ = fs::remove_dir_all(&tmp);
    }
}
