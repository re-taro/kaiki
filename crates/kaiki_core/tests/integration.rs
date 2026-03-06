use std::fs;
use std::path::PathBuf;

use compact_str::CompactString;
use kaiki_config::CoreConfig;
use kaiki_core::processor::RegProcessor;
use kaiki_git::SimpleKeygen;

/// Create a minimal 2x2 red PNG in memory.
fn make_red_png() -> Vec<u8> {
    let mut buf = Vec::new();
    let img = image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255]));
    {
        use image::ImageEncoder;
        let encoder = image::codecs::png::PngEncoder::new(&mut buf);
        encoder.write_image(img.as_raw(), 2, 2, image::ExtendedColorType::Rgba8).unwrap();
    }
    buf
}

/// Create a minimal 2x2 blue PNG in memory.
fn make_blue_png() -> Vec<u8> {
    let mut buf = Vec::new();
    let img = image::RgbaImage::from_pixel(2, 2, image::Rgba([0, 0, 255, 255]));
    {
        use image::ImageEncoder;
        let encoder = image::codecs::png::PngEncoder::new(&mut buf);
        encoder.write_image(img.as_raw(), 2, 2, image::ExtendedColorType::Rgba8).unwrap();
    }
    buf
}

/// Set up a test fixture: actual + expected dirs with images.
fn setup_fixture(
    tmpdir: &std::path::Path,
    actual_images: &[(&str, &[u8])],
    expected_images: &[(&str, &[u8])],
) -> (PathBuf, PathBuf) {
    let actual_dir = tmpdir.join("actual");
    let expected_dir = tmpdir.join("working").join("expected");

    fs::create_dir_all(&actual_dir).unwrap();
    fs::create_dir_all(&expected_dir).unwrap();

    for (name, data) in actual_images {
        fs::write(actual_dir.join(name), data).unwrap();
    }
    for (name, data) in expected_images {
        fs::write(expected_dir.join(name), data).unwrap();
    }

    (actual_dir, tmpdir.join("working"))
}

#[test]
fn test_compare_with_fixture_images() {
    let red = make_red_png();
    let blue = make_blue_png();
    let tmpdir = tempfile::tempdir().unwrap();

    let (actual_dir, working_dir) = setup_fixture(
        tmpdir.path(),
        &[("same.png", &red), ("diff.png", &red)],
        &[("same.png", &red), ("diff.png", &blue)],
    );

    let config = CoreConfig {
        actual_dir: actual_dir.to_string_lossy().to_string(),
        ..CoreConfig::default()
    };
    let keygen = SimpleKeygen { expected_key: "test-key".to_string() };

    let processor = RegProcessor::new(
        config,
        working_dir,
        Box::new(keygen),
        None,
        vec![],
    );

    let result = processor.compare().unwrap();

    // "same.png" should pass (identical images)
    assert!(result.passed_items.contains(&CompactString::from("same.png")));
    // "diff.png" should fail (red vs blue)
    assert!(result.failed_items.contains(&CompactString::from("diff.png")));
    // No new or deleted items
    assert!(result.new_items.is_empty());
    assert!(result.deleted_items.is_empty());
}

#[test]
fn test_compare_new_and_deleted_items() {
    let red = make_red_png();
    let blue = make_blue_png();
    let tmpdir = tempfile::tempdir().unwrap();

    let (actual_dir, working_dir) = setup_fixture(
        tmpdir.path(),
        &[("actual_only.png", &red)],
        &[("expected_only.png", &blue)],
    );

    let config = CoreConfig {
        actual_dir: actual_dir.to_string_lossy().to_string(),
        ..CoreConfig::default()
    };
    let keygen = SimpleKeygen { expected_key: "test-key".to_string() };

    let processor = RegProcessor::new(
        config,
        working_dir,
        Box::new(keygen),
        None,
        vec![],
    );

    let result = processor.compare().unwrap();

    assert!(result.new_items.contains(&CompactString::from("actual_only.png")));
    assert!(result.deleted_items.contains(&CompactString::from("expected_only.png")));
    assert!(result.failed_items.is_empty());
    assert!(result.passed_items.is_empty());
}

#[test]
fn test_report_generation() {
    let red = make_red_png();
    let tmpdir = tempfile::tempdir().unwrap();

    let (actual_dir, working_dir) = setup_fixture(
        tmpdir.path(),
        &[("img.png", &red)],
        &[("img.png", &red)],
    );

    let config = CoreConfig {
        actual_dir: actual_dir.to_string_lossy().to_string(),
        ..CoreConfig::default()
    };
    let keygen = SimpleKeygen { expected_key: "test-key".to_string() };

    let processor = RegProcessor::new(
        config,
        working_dir.clone(),
        Box::new(keygen),
        None,
        vec![],
    );

    let _result = processor.compare().unwrap();

    // Verify out.json was created
    let json_path = working_dir.join("out.json");
    assert!(json_path.exists(), "out.json should be generated");
    let json_content = fs::read_to_string(&json_path).unwrap();
    assert!(json_content.contains("passedItems"));

    // Verify index.html was created
    let html_path = working_dir.join("index.html");
    assert!(html_path.exists(), "index.html should be generated");
    let html_content = fs::read_to_string(&html_path).unwrap();
    assert!(html_content.contains("reg report"));
    assert!(html_content.contains("reg-data"));
}
