use std::fs;
use std::path::{Path, PathBuf};

use compact_str::CompactString;
use kaiki_config::CoreConfig;
use kaiki_core::processor::RegProcessor;
use kaiki_git::SimpleKeygen;

/// Create a solid-color PNG of the given dimensions.
pub fn make_solid_png(w: u32, h: u32, color: [u8; 4]) -> Vec<u8> {
    let mut buf = Vec::new();
    let img = image::RgbaImage::from_pixel(w, h, image::Rgba(color));
    {
        use image::ImageEncoder;
        let encoder = image::codecs::png::PngEncoder::new(&mut buf);
        encoder.write_image(img.as_raw(), w, h, image::ExtendedColorType::Rgba8).unwrap();
    }
    buf
}

/// Create a solid-color PNG where exactly one pixel differs.
/// The pixel at (0, 0) is `diff_color`; all others are `base_color`.
pub fn make_mostly_solid_png(w: u32, h: u32, base_color: [u8; 4], diff_color: [u8; 4]) -> Vec<u8> {
    let mut img = image::RgbaImage::from_pixel(w, h, image::Rgba(base_color));
    img.put_pixel(0, 0, image::Rgba(diff_color));
    let mut buf = Vec::new();
    {
        use image::ImageEncoder;
        let encoder = image::codecs::png::PngEncoder::new(&mut buf);
        encoder.write_image(img.as_raw(), w, h, image::ExtendedColorType::Rgba8).unwrap();
    }
    buf
}

/// Set up a test fixture with actual and expected directories.
/// Supports nested paths (e.g. `"buttons/primary.png"`).
pub fn setup_fixture(
    tmpdir: &Path,
    actual_images: &[(&str, &[u8])],
    expected_images: &[(&str, &[u8])],
) -> (PathBuf, PathBuf) {
    let actual_dir = tmpdir.join("actual");
    let expected_dir = tmpdir.join("working").join("expected");

    fs::create_dir_all(&actual_dir).unwrap();
    fs::create_dir_all(&expected_dir).unwrap();

    for (name, data) in actual_images {
        let path = actual_dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, data).unwrap();
    }
    for (name, data) in expected_images {
        let path = expected_dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, data).unwrap();
    }

    (actual_dir, tmpdir.join("working"))
}

/// Build a `RegProcessor` with `concurrency: Some(1)` for deterministic ordering.
pub fn make_processor(actual_dir: &Path, working_dir: PathBuf) -> RegProcessor {
    make_processor_with_config(actual_dir, working_dir, |_| {})
}

/// Build a `RegProcessor` with customizable config.
/// The callback receives a `&mut CoreConfig` after defaults are applied.
pub fn make_processor_with_config<F: FnOnce(&mut CoreConfig)>(
    actual_dir: &Path,
    working_dir: PathBuf,
    config_fn: F,
) -> RegProcessor {
    let mut config = CoreConfig {
        actual_dir: actual_dir.to_string_lossy().to_string(),
        concurrency: Some(1),
        ..CoreConfig::default()
    };
    config_fn(&mut config);
    let keygen = SimpleKeygen { expected_key: "test-key".to_string() };
    RegProcessor::new(config, working_dir, Box::new(keygen), None, vec![])
}

/// Read `out.json` from the working directory and parse it as `serde_json::Value`.
pub fn read_out_json(working_dir: &Path) -> serde_json::Value {
    let json_str = fs::read_to_string(working_dir.join("out.json")).unwrap();
    serde_json::from_str(&json_str).unwrap()
}

/// Extract an array field from a JSON value and return sorted string entries.
pub fn sorted_json_array(value: &serde_json::Value, field: &str) -> Vec<String> {
    let mut items: Vec<String> = value[field]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    items.sort();
    items
}

/// Sort a slice of `CompactString` and return a new `Vec`.
pub fn sorted(items: &[CompactString]) -> Vec<CompactString> {
    let mut v = items.to_vec();
    v.sort();
    v
}
