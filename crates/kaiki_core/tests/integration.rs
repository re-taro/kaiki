mod common;

use compact_str::CompactString;
use kaiki_config::CoreConfig;
use kaiki_core::processor::RegProcessor;
use kaiki_git::SimpleKeygen;

use common::{make_solid_png, setup_fixture};

const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];

#[test]
fn test_compare_with_fixture_images() {
    let red = make_solid_png(2, 2, RED);
    let blue = make_solid_png(2, 2, BLUE);
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

    let processor = RegProcessor::new(config, working_dir, Box::new(keygen), None, vec![]);

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
    let red = make_solid_png(2, 2, RED);
    let blue = make_solid_png(2, 2, BLUE);
    let tmpdir = tempfile::tempdir().unwrap();

    let (actual_dir, working_dir) =
        setup_fixture(tmpdir.path(), &[("actual_only.png", &red)], &[("expected_only.png", &blue)]);

    let config = CoreConfig {
        actual_dir: actual_dir.to_string_lossy().to_string(),
        ..CoreConfig::default()
    };
    let keygen = SimpleKeygen { expected_key: "test-key".to_string() };

    let processor = RegProcessor::new(config, working_dir, Box::new(keygen), None, vec![]);

    let result = processor.compare().unwrap();

    assert!(result.new_items.contains(&CompactString::from("actual_only.png")));
    assert!(result.deleted_items.contains(&CompactString::from("expected_only.png")));
    assert!(result.failed_items.is_empty());
    assert!(result.passed_items.is_empty());
}

#[test]
fn test_report_generation() {
    let red = make_solid_png(2, 2, RED);
    let tmpdir = tempfile::tempdir().unwrap();

    let (actual_dir, working_dir) =
        setup_fixture(tmpdir.path(), &[("img.png", &red)], &[("img.png", &red)]);

    let config = CoreConfig {
        actual_dir: actual_dir.to_string_lossy().to_string(),
        ..CoreConfig::default()
    };
    let keygen = SimpleKeygen { expected_key: "test-key".to_string() };

    let processor = RegProcessor::new(config, working_dir.clone(), Box::new(keygen), None, vec![]);

    let _result = processor.compare().unwrap();

    // Verify out.json was created
    let json_path = working_dir.join("out.json");
    assert!(json_path.exists(), "out.json should be generated");
    let json_content = std::fs::read_to_string(&json_path).unwrap();
    assert!(json_content.contains("passedItems"));

    // Verify index.html was created
    let html_path = working_dir.join("index.html");
    assert!(html_path.exists(), "index.html should be generated");
    let html_content = std::fs::read_to_string(&html_path).unwrap();
    assert!(html_content.contains("reg report"));
    assert!(html_content.contains("reg-data"));
}
