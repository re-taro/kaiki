//! reg-suit compatibility E2E tests.
//!
//! These tests verify that kaiki produces output compatible with reg-suit:
//! out.json schema, category classification, threshold logic, diff image
//! generation, nested directory handling, edge cases, and report files.

mod common;

use std::fs;

use compact_str::CompactString;
use kaiki_report::ComparisonResult;

use common::{
    make_mostly_solid_png, make_processor, make_processor_with_config, make_solid_png,
    read_out_json, setup_fixture, sorted, sorted_json_array,
};

const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];

// ───────────────────────────────────────────────────
// Group 1: out.json schema compatibility
// ───────────────────────────────────────────────────

#[test]
fn out_json_has_all_ten_camel_case_fields() {
    let red = make_solid_png(2, 2, RED);
    let tmpdir = tempfile::tempdir().unwrap();
    let (actual_dir, working_dir) =
        setup_fixture(tmpdir.path(), &[("a.png", &red)], &[("a.png", &red)]);

    let processor = make_processor(&actual_dir, working_dir.clone());
    processor.compare().unwrap();

    let json = read_out_json(&working_dir);
    let obj = json.as_object().unwrap();

    let expected_fields = [
        "failedItems",
        "newItems",
        "deletedItems",
        "passedItems",
        "expectedItems",
        "actualItems",
        "diffItems",
        "actualDir",
        "expectedDir",
        "diffDir",
    ];
    for field in &expected_fields {
        assert!(obj.contains_key(*field), "missing field: {field}");
    }
    assert_eq!(obj.len(), 10, "expected exactly 10 fields, got {}", obj.len());
}

#[test]
fn out_json_items_are_relative_paths() {
    let red = make_solid_png(2, 2, RED);
    let blue = make_solid_png(2, 2, BLUE);
    let tmpdir = tempfile::tempdir().unwrap();
    let (actual_dir, working_dir) = setup_fixture(
        tmpdir.path(),
        &[("pass.png", &red), ("fail.png", &red), ("new.png", &red)],
        &[("pass.png", &red), ("fail.png", &blue), ("del.png", &red)],
    );

    let processor = make_processor(&actual_dir, working_dir.clone());
    processor.compare().unwrap();

    let json = read_out_json(&working_dir);
    let arrays = [
        "failedItems",
        "newItems",
        "deletedItems",
        "passedItems",
        "expectedItems",
        "actualItems",
        "diffItems",
    ];
    for field in &arrays {
        for item in json[field].as_array().unwrap() {
            let s = item.as_str().unwrap();
            assert!(!s.starts_with('/'), "{field} item {s:?} is an absolute path");
        }
    }
}

#[test]
fn out_json_directory_fields_are_relative() {
    let red = make_solid_png(2, 2, RED);
    let tmpdir = tempfile::tempdir().unwrap();
    let (actual_dir, working_dir) =
        setup_fixture(tmpdir.path(), &[("a.png", &red)], &[("a.png", &red)]);

    let processor = make_processor(&actual_dir, working_dir.clone());
    processor.compare().unwrap();

    let json = read_out_json(&working_dir);
    assert_eq!(json["actualDir"].as_str().unwrap(), "actual");
    assert_eq!(json["expectedDir"].as_str().unwrap(), "expected");
    assert_eq!(json["diffDir"].as_str().unwrap(), "diff");
}

#[test]
fn out_json_deserializes_to_comparison_result() {
    let red = make_solid_png(2, 2, RED);
    let blue = make_solid_png(2, 2, BLUE);
    let tmpdir = tempfile::tempdir().unwrap();
    let (actual_dir, working_dir) = setup_fixture(
        tmpdir.path(),
        &[("pass.png", &red), ("fail.png", &red)],
        &[("pass.png", &red), ("fail.png", &blue)],
    );

    let processor = make_processor(&actual_dir, working_dir.clone());
    let original = processor.compare().unwrap();

    let json_str = fs::read_to_string(working_dir.join("out.json")).unwrap();
    let deserialized: ComparisonResult = serde_json::from_str(&json_str).unwrap();

    assert_eq!(sorted(&original.failed_items), sorted(&deserialized.failed_items));
    assert_eq!(sorted(&original.passed_items), sorted(&deserialized.passed_items));
    assert_eq!(sorted(&original.new_items), sorted(&deserialized.new_items));
    assert_eq!(sorted(&original.deleted_items), sorted(&deserialized.deleted_items));
    assert_eq!(sorted(&original.diff_items), sorted(&deserialized.diff_items));
    assert_eq!(original.actual_dir, deserialized.actual_dir);
    assert_eq!(original.expected_dir, deserialized.expected_dir);
    assert_eq!(original.diff_dir, deserialized.diff_dir);
}

// ───────────────────────────────────────────────────
// Group 2: Category classification
// ───────────────────────────────────────────────────

#[test]
fn identical_images_go_to_passed_items() {
    let red = make_solid_png(2, 2, RED);
    let tmpdir = tempfile::tempdir().unwrap();
    let (actual_dir, working_dir) =
        setup_fixture(tmpdir.path(), &[("img.png", &red)], &[("img.png", &red)]);

    let processor = make_processor(&actual_dir, working_dir);
    let result = processor.compare().unwrap();

    assert_eq!(result.passed_items, vec![CompactString::from("img.png")]);
    assert!(result.failed_items.is_empty());
    assert!(result.new_items.is_empty());
    assert!(result.deleted_items.is_empty());
}

#[test]
fn different_images_go_to_failed_items() {
    let red = make_solid_png(2, 2, RED);
    let blue = make_solid_png(2, 2, BLUE);
    let tmpdir = tempfile::tempdir().unwrap();
    let (actual_dir, working_dir) =
        setup_fixture(tmpdir.path(), &[("img.png", &red)], &[("img.png", &blue)]);

    let processor = make_processor(&actual_dir, working_dir);
    let result = processor.compare().unwrap();

    assert_eq!(result.failed_items, vec![CompactString::from("img.png")]);
    assert!(result.passed_items.is_empty());
}

#[test]
fn actual_only_images_are_new_items() {
    let red = make_solid_png(2, 2, RED);
    let tmpdir = tempfile::tempdir().unwrap();
    let (actual_dir, working_dir) =
        setup_fixture(tmpdir.path(), &[("brand_new.png", &red)], &[]);

    let processor = make_processor(&actual_dir, working_dir);
    let result = processor.compare().unwrap();

    assert_eq!(result.new_items, vec![CompactString::from("brand_new.png")]);
    assert!(result.passed_items.is_empty());
    assert!(result.failed_items.is_empty());
    assert!(result.deleted_items.is_empty());
}

#[test]
fn expected_only_images_are_deleted_items() {
    let red = make_solid_png(2, 2, RED);
    let tmpdir = tempfile::tempdir().unwrap();
    let (actual_dir, working_dir) =
        setup_fixture(tmpdir.path(), &[], &[("gone.png", &red)]);

    let processor = make_processor(&actual_dir, working_dir);
    let result = processor.compare().unwrap();

    assert_eq!(result.deleted_items, vec![CompactString::from("gone.png")]);
    assert!(result.passed_items.is_empty());
    assert!(result.failed_items.is_empty());
    assert!(result.new_items.is_empty());
}

#[test]
fn mixed_scenario_all_categories() {
    let red = make_solid_png(2, 2, RED);
    let blue = make_solid_png(2, 2, BLUE);
    let tmpdir = tempfile::tempdir().unwrap();
    let (actual_dir, working_dir) = setup_fixture(
        tmpdir.path(),
        &[("pass.png", &red), ("fail.png", &red), ("new.png", &red)],
        &[("pass.png", &red), ("fail.png", &blue), ("del.png", &red)],
    );

    let processor = make_processor(&actual_dir, working_dir.clone());
    let result = processor.compare().unwrap();

    assert_eq!(sorted(&result.passed_items), vec![CompactString::from("pass.png")]);
    assert_eq!(sorted(&result.failed_items), vec![CompactString::from("fail.png")]);
    assert_eq!(sorted(&result.new_items), vec![CompactString::from("new.png")]);
    assert_eq!(sorted(&result.deleted_items), vec![CompactString::from("del.png")]);

    // diff_items = ALL common items (passed + failed), reg-suit compatible
    let mut expected_diff: Vec<CompactString> =
        vec![CompactString::from("fail.png"), CompactString::from("pass.png")];
    expected_diff.sort();
    assert_eq!(sorted(&result.diff_items), expected_diff);
}

#[test]
fn empty_case_no_images() {
    let tmpdir = tempfile::tempdir().unwrap();
    let (actual_dir, working_dir) = setup_fixture(tmpdir.path(), &[], &[]);

    let processor = make_processor(&actual_dir, working_dir.clone());
    let result = processor.compare().unwrap();

    assert!(result.passed_items.is_empty());
    assert!(result.failed_items.is_empty());
    assert!(result.new_items.is_empty());
    assert!(result.deleted_items.is_empty());
    assert!(result.diff_items.is_empty());

    // out.json should still be valid
    let json = read_out_json(&working_dir);
    assert!(json.is_object());
}

// ───────────────────────────────────────────────────
// Group 3: Threshold logic
// ───────────────────────────────────────────────────

// 10x10 images (100 pixels) with 1 pixel different.
const THRESH_W: u32 = 10;
const THRESH_H: u32 = 10;

fn make_threshold_pair() -> (Vec<u8>, Vec<u8>) {
    let base = make_solid_png(THRESH_W, THRESH_H, RED);
    let diff = make_mostly_solid_png(THRESH_W, THRESH_H, RED, BLUE);
    (base, diff)
}

#[test]
fn no_threshold_only_exact_match_passes() {
    let (base, diff) = make_threshold_pair();
    let tmpdir = tempfile::tempdir().unwrap();
    let (actual_dir, working_dir) =
        setup_fixture(tmpdir.path(), &[("img.png", &diff)], &[("img.png", &base)]);

    let processor = make_processor(&actual_dir, working_dir);
    let result = processor.compare().unwrap();

    assert_eq!(result.failed_items, vec![CompactString::from("img.png")]);
    assert!(result.passed_items.is_empty());
}

#[test]
fn threshold_rate_small_diff_within_rate_passes() {
    // 1 pixel / 100 = 0.01 (1%) < threshold_rate 0.02 (2%) → pass
    let (base, diff) = make_threshold_pair();
    let tmpdir = tempfile::tempdir().unwrap();
    let (actual_dir, working_dir) =
        setup_fixture(tmpdir.path(), &[("img.png", &diff)], &[("img.png", &base)]);

    let processor = make_processor_with_config(&actual_dir, working_dir, |c| {
        c.threshold_rate = Some(0.02);
    });
    let result = processor.compare().unwrap();

    assert_eq!(result.passed_items, vec![CompactString::from("img.png")]);
    assert!(result.failed_items.is_empty());
}

#[test]
fn threshold_rate_small_diff_exceeding_rate_fails() {
    // 1 pixel / 100 = 0.01 (1%) > threshold_rate 0.005 (0.5%) → fail
    let (base, diff) = make_threshold_pair();
    let tmpdir = tempfile::tempdir().unwrap();
    let (actual_dir, working_dir) =
        setup_fixture(tmpdir.path(), &[("img.png", &diff)], &[("img.png", &base)]);

    let processor = make_processor_with_config(&actual_dir, working_dir, |c| {
        c.threshold_rate = Some(0.005);
    });
    let result = processor.compare().unwrap();

    assert_eq!(result.failed_items, vec![CompactString::from("img.png")]);
    assert!(result.passed_items.is_empty());
}

#[test]
fn threshold_pixel_within_count_passes() {
    // 1 diff pixel <= threshold_pixel 5 → pass
    let (base, diff) = make_threshold_pair();
    let tmpdir = tempfile::tempdir().unwrap();
    let (actual_dir, working_dir) =
        setup_fixture(tmpdir.path(), &[("img.png", &diff)], &[("img.png", &base)]);

    let processor = make_processor_with_config(&actual_dir, working_dir, |c| {
        c.threshold_pixel = Some(5);
    });
    let result = processor.compare().unwrap();

    assert_eq!(result.passed_items, vec![CompactString::from("img.png")]);
    assert!(result.failed_items.is_empty());
}

#[test]
fn threshold_pixel_takes_precedence_over_rate() {
    // threshold_pixel=0 (strict) + threshold_rate=0.5 (lenient)
    // pixel takes precedence → 1 diff > 0 → fail (reg-suit compatible)
    let (base, diff) = make_threshold_pair();
    let tmpdir = tempfile::tempdir().unwrap();
    let (actual_dir, working_dir) =
        setup_fixture(tmpdir.path(), &[("img.png", &diff)], &[("img.png", &base)]);

    let processor = make_processor_with_config(&actual_dir, working_dir, |c| {
        c.threshold_pixel = Some(0);
        c.threshold_rate = Some(0.5);
    });
    let result = processor.compare().unwrap();

    assert_eq!(result.failed_items, vec![CompactString::from("img.png")]);
    assert!(result.passed_items.is_empty());
}

#[test]
fn threshold_legacy_alias_for_threshold_rate() {
    // `threshold` field should behave as an alias for `threshold_rate`
    let (base, diff) = make_threshold_pair();
    let tmpdir = tempfile::tempdir().unwrap();
    let (actual_dir, working_dir) =
        setup_fixture(tmpdir.path(), &[("img.png", &diff)], &[("img.png", &base)]);

    let processor = make_processor_with_config(&actual_dir, working_dir, |c| {
        c.threshold = Some(0.02); // legacy alias
        c.threshold_rate = None;
    });
    let result = processor.compare().unwrap();

    assert_eq!(result.passed_items, vec![CompactString::from("img.png")]);
    assert!(result.failed_items.is_empty());
}

// ───────────────────────────────────────────────────
// Group 4: Diff image generation
// ───────────────────────────────────────────────────

#[test]
fn diff_images_generated_for_failed_items() {
    let red = make_solid_png(2, 2, RED);
    let blue = make_solid_png(2, 2, BLUE);
    let tmpdir = tempfile::tempdir().unwrap();
    let (actual_dir, working_dir) =
        setup_fixture(tmpdir.path(), &[("fail.png", &red)], &[("fail.png", &blue)]);

    let processor = make_processor(&actual_dir, working_dir.clone());
    processor.compare().unwrap();

    let diff_path = working_dir.join("diff").join("fail.png");
    assert!(diff_path.exists(), "diff image should exist for failed item");
}

#[test]
fn diff_images_are_valid_png_with_correct_dimensions() {
    let red = make_solid_png(10, 8, RED);
    let blue = make_solid_png(10, 8, BLUE);
    let tmpdir = tempfile::tempdir().unwrap();
    let (actual_dir, working_dir) =
        setup_fixture(tmpdir.path(), &[("img.png", &red)], &[("img.png", &blue)]);

    let processor = make_processor(&actual_dir, working_dir.clone());
    processor.compare().unwrap();

    let diff_bytes = fs::read(working_dir.join("diff").join("img.png")).unwrap();
    let diff_img = image::load_from_memory(&diff_bytes).unwrap();
    assert_eq!(diff_img.width(), 10);
    assert_eq!(diff_img.height(), 8);
}

#[test]
fn no_diff_image_for_hash_identical_passed_items() {
    // Hash fast path: identical files → no diff image generated
    let red = make_solid_png(2, 2, RED);
    let tmpdir = tempfile::tempdir().unwrap();
    let (actual_dir, working_dir) =
        setup_fixture(tmpdir.path(), &[("same.png", &red)], &[("same.png", &red)]);

    let processor = make_processor(&actual_dir, working_dir.clone());
    let result = processor.compare().unwrap();

    assert!(result.passed_items.contains(&CompactString::from("same.png")));
    let diff_path = working_dir.join("diff").join("same.png");
    assert!(!diff_path.exists(), "no diff image should be generated for hash-identical files");
}

#[test]
fn no_diff_images_for_new_or_deleted_items() {
    let red = make_solid_png(2, 2, RED);
    let tmpdir = tempfile::tempdir().unwrap();
    let (actual_dir, working_dir) =
        setup_fixture(tmpdir.path(), &[("new.png", &red)], &[("del.png", &red)]);

    let processor = make_processor(&actual_dir, working_dir.clone());
    processor.compare().unwrap();

    assert!(!working_dir.join("diff").join("new.png").exists());
    assert!(!working_dir.join("diff").join("del.png").exists());
}

#[test]
fn diff_items_list_equals_all_common_items() {
    let red = make_solid_png(2, 2, RED);
    let blue = make_solid_png(2, 2, BLUE);
    let tmpdir = tempfile::tempdir().unwrap();
    let (actual_dir, working_dir) = setup_fixture(
        tmpdir.path(),
        &[("pass.png", &red), ("fail.png", &red), ("new.png", &red)],
        &[("pass.png", &red), ("fail.png", &blue), ("del.png", &red)],
    );

    let processor = make_processor(&actual_dir, working_dir.clone());
    let result = processor.compare().unwrap();

    // diff_items should contain ALL common items (both passed and failed)
    let mut expected_common =
        vec![CompactString::from("fail.png"), CompactString::from("pass.png")];
    expected_common.sort();
    assert_eq!(sorted(&result.diff_items), expected_common);

    // Also verify via out.json
    let json = read_out_json(&working_dir);
    let diff_json = sorted_json_array(&json, "diffItems");
    assert_eq!(diff_json, vec!["fail.png", "pass.png"]);
}

// ───────────────────────────────────────────────────
// Group 5: Nested directory support
// ───────────────────────────────────────────────────

#[test]
fn images_in_subdirectories_preserved() {
    let red = make_solid_png(2, 2, RED);
    let blue = make_solid_png(2, 2, BLUE);
    let tmpdir = tempfile::tempdir().unwrap();
    let (actual_dir, working_dir) = setup_fixture(
        tmpdir.path(),
        &[("buttons/primary.png", &red)],
        &[("buttons/primary.png", &blue)],
    );

    let processor = make_processor(&actual_dir, working_dir.clone());
    let result = processor.compare().unwrap();

    assert!(result.failed_items.contains(&CompactString::from("buttons/primary.png")));

    let json = read_out_json(&working_dir);
    let failed = sorted_json_array(&json, "failedItems");
    assert!(failed.contains(&"buttons/primary.png".to_string()));
}

#[test]
fn deep_nesting_works_correctly() {
    let red = make_solid_png(2, 2, RED);
    let tmpdir = tempfile::tempdir().unwrap();
    let (actual_dir, working_dir) = setup_fixture(
        tmpdir.path(),
        &[("components/buttons/primary/default.png", &red)],
        &[("components/buttons/primary/default.png", &red)],
    );

    let processor = make_processor(&actual_dir, working_dir.clone());
    let result = processor.compare().unwrap();

    assert!(result
        .passed_items
        .contains(&CompactString::from("components/buttons/primary/default.png")));

    let json = read_out_json(&working_dir);
    let passed = sorted_json_array(&json, "passedItems");
    assert!(passed.contains(&"components/buttons/primary/default.png".to_string()));
}

#[test]
fn nested_diff_images_created_in_correct_subdirectory() {
    let red = make_solid_png(2, 2, RED);
    let blue = make_solid_png(2, 2, BLUE);
    let tmpdir = tempfile::tempdir().unwrap();
    let (actual_dir, working_dir) = setup_fixture(
        tmpdir.path(),
        &[("buttons/primary.png", &red)],
        &[("buttons/primary.png", &blue)],
    );

    let processor = make_processor(&actual_dir, working_dir.clone());
    processor.compare().unwrap();

    let diff_path = working_dir.join("diff").join("buttons").join("primary.png");
    assert!(diff_path.exists(), "diff image should be in nested subdirectory");
}

// ───────────────────────────────────────────────────
// Group 6: Edge cases
// ───────────────────────────────────────────────────

#[test]
fn single_image_pass() {
    let red = make_solid_png(2, 2, RED);
    let tmpdir = tempfile::tempdir().unwrap();
    let (actual_dir, working_dir) =
        setup_fixture(tmpdir.path(), &[("only.png", &red)], &[("only.png", &red)]);

    let processor = make_processor(&actual_dir, working_dir);
    let result = processor.compare().unwrap();

    assert_eq!(result.passed_items.len(), 1);
    assert!(!result.has_failures());
}

#[test]
fn single_image_fail() {
    let red = make_solid_png(2, 2, RED);
    let blue = make_solid_png(2, 2, BLUE);
    let tmpdir = tempfile::tempdir().unwrap();
    let (actual_dir, working_dir) =
        setup_fixture(tmpdir.path(), &[("only.png", &red)], &[("only.png", &blue)]);

    let processor = make_processor(&actual_dir, working_dir);
    let result = processor.compare().unwrap();

    assert_eq!(result.failed_items.len(), 1);
    assert!(result.has_failures());
}

#[test]
fn only_new_images_no_expected() {
    let red = make_solid_png(2, 2, RED);
    let tmpdir = tempfile::tempdir().unwrap();
    let (actual_dir, working_dir) = setup_fixture(
        tmpdir.path(),
        &[("a.png", &red), ("b.png", &red), ("c.png", &red)],
        &[],
    );

    let processor = make_processor(&actual_dir, working_dir);
    let result = processor.compare().unwrap();

    assert_eq!(sorted(&result.new_items).len(), 3);
    assert!(result.passed_items.is_empty());
    assert!(result.failed_items.is_empty());
    assert!(result.deleted_items.is_empty());
}

#[test]
fn only_deleted_images_no_actual_matching() {
    let red = make_solid_png(2, 2, RED);
    let tmpdir = tempfile::tempdir().unwrap();
    let (actual_dir, working_dir) = setup_fixture(
        tmpdir.path(),
        &[],
        &[("x.png", &red), ("y.png", &red), ("z.png", &red)],
    );

    let processor = make_processor(&actual_dir, working_dir);
    let result = processor.compare().unwrap();

    assert_eq!(sorted(&result.deleted_items).len(), 3);
    assert!(result.passed_items.is_empty());
    assert!(result.failed_items.is_empty());
    assert!(result.new_items.is_empty());
}

#[test]
fn many_images_deterministic() {
    let red = make_solid_png(2, 2, RED);
    let blue = make_solid_png(2, 2, BLUE);
    let tmpdir = tempfile::tempdir().unwrap();

    let mut actual_images: Vec<(String, Vec<u8>)> = Vec::new();
    let mut expected_images: Vec<(String, Vec<u8>)> = Vec::new();

    // 25 pass + 25 fail = 50 images
    for i in 0..25 {
        let name = format!("pass_{i:03}.png");
        actual_images.push((name.clone(), red.clone()));
        expected_images.push((name, red.clone()));
    }
    for i in 0..25 {
        let name = format!("fail_{i:03}.png");
        actual_images.push((name.clone(), red.clone()));
        expected_images.push((name, blue.clone()));
    }

    let actual_refs: Vec<(&str, &[u8])> =
        actual_images.iter().map(|(n, d)| (n.as_str(), d.as_slice())).collect();
    let expected_refs: Vec<(&str, &[u8])> =
        expected_images.iter().map(|(n, d)| (n.as_str(), d.as_slice())).collect();

    let (actual_dir, working_dir) =
        setup_fixture(tmpdir.path(), &actual_refs, &expected_refs);

    let processor = make_processor(&actual_dir, working_dir);
    let result = processor.compare().unwrap();

    assert_eq!(result.passed_items.len(), 25);
    assert_eq!(result.failed_items.len(), 25);
    assert_eq!(result.diff_items.len(), 50);
    assert!(result.new_items.is_empty());
    assert!(result.deleted_items.is_empty());

    // Verify all pass/fail names
    let passed = sorted(&result.passed_items);
    let failed = sorted(&result.failed_items);
    for i in 0..25 {
        assert!(passed.contains(&CompactString::from(format!("pass_{i:03}.png"))));
        assert!(failed.contains(&CompactString::from(format!("fail_{i:03}.png"))));
    }
}

// ───────────────────────────────────────────────────
// Group 7: Report generation
// ───────────────────────────────────────────────────

#[test]
fn out_json_is_pretty_printed() {
    let red = make_solid_png(2, 2, RED);
    let tmpdir = tempfile::tempdir().unwrap();
    let (actual_dir, working_dir) =
        setup_fixture(tmpdir.path(), &[("a.png", &red)], &[("a.png", &red)]);

    let processor = make_processor(&actual_dir, working_dir.clone());
    processor.compare().unwrap();

    let json_str = fs::read_to_string(working_dir.join("out.json")).unwrap();
    assert!(json_str.contains('\n'), "out.json should be pretty-printed with newlines");
    assert!(json_str.contains("  "), "out.json should be indented");
}

#[test]
fn index_html_exists_with_expected_markers() {
    let red = make_solid_png(2, 2, RED);
    let tmpdir = tempfile::tempdir().unwrap();
    let (actual_dir, working_dir) =
        setup_fixture(tmpdir.path(), &[("a.png", &red)], &[("a.png", &red)]);

    let processor = make_processor(&actual_dir, working_dir.clone());
    processor.compare().unwrap();

    let html_path = working_dir.join("index.html");
    assert!(html_path.exists());
    let html = fs::read_to_string(&html_path).unwrap();
    assert!(html.contains("reg report"), "index.html should contain 'reg report'");
    assert!(html.contains("reg-data"), "index.html should contain 'reg-data'");
}

#[test]
fn report_reflects_failure_status() {
    let red = make_solid_png(2, 2, RED);
    let blue = make_solid_png(2, 2, BLUE);
    let tmpdir = tempfile::tempdir().unwrap();
    let (actual_dir, working_dir) =
        setup_fixture(tmpdir.path(), &[("fail.png", &red)], &[("fail.png", &blue)]);

    let processor = make_processor(&actual_dir, working_dir);
    let result = processor.compare().unwrap();

    assert!(result.has_failures());
    assert!(!result.failed_items.is_empty());
}
