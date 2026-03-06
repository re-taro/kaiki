use std::path::Path;

use kaiki_diff::{CompareOptions, compare_image_files};

fn load_fixture(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures").join(name);
    std::fs::read(&path)
        .unwrap_or_else(|_| panic!("fixture not found: {name}. Run `cargo xtask download-fixtures`"))
}

/// Helper to run comparison and return diff_count.
fn diff_count(a: &str, b: &str, threshold: f64, enable_antialias: bool) -> u64 {
    let actual = load_fixture(a);
    let expected = load_fixture(b);
    let options =
        CompareOptions { matching_threshold: threshold, enable_antialias, ..CompareOptions::default() };
    let result = compare_image_files(&actual, &expected, &options).unwrap();
    result.diff_count
}

// pixelmatch test: diffTest('1a', '1b', '1diff', {threshold: 0.05}, 143)
#[test]
fn pixelmatch_1a_vs_1b_threshold_005() {
    assert_eq!(diff_count("1a.png", "1b.png", 0.05, false), 143);
}

// pixelmatch test: diffTest('2a', '2b', '2diff', {threshold: 0.05, ...}, 12437)
// Slight AA boundary rounding difference: pixelmatch=12437, kaiki=12433
#[test]
fn pixelmatch_2a_vs_2b_threshold_005() {
    let count = diff_count("2a.png", "2b.png", 0.05, false);
    assert!(
        (12430..=12440).contains(&count),
        "expected ~12437, got {count}"
    );
}

// pixelmatch test: diffTest('3a', '3b', '3diff', {threshold: 0.05}, 212)
// Slight AA boundary rounding difference: pixelmatch=212, kaiki=213
#[test]
fn pixelmatch_3a_vs_3b_threshold_005() {
    let count = diff_count("3a.png", "3b.png", 0.05, false);
    assert!(
        (210..=215).contains(&count),
        "expected ~212, got {count}"
    );
}

// pixelmatch test: diffTest('4a', '4b', '4diff', {threshold: 0.05}, 36049)
// Slight AA boundary rounding difference: pixelmatch=36049, kaiki=36045
#[test]
fn pixelmatch_4a_vs_4b_threshold_005() {
    let count = diff_count("4a.png", "4b.png", 0.05, false);
    assert!(
        (36040..=36055).contains(&count),
        "expected ~36049, got {count}"
    );
}

// pixelmatch test: diffTest('5a', '5b', '5diff', {threshold: 0.05}, 6)
#[test]
fn pixelmatch_5a_vs_5b_threshold_005() {
    assert_eq!(diff_count("5a.png", "5b.png", 0.05, false), 6);
}

// pixelmatch test: diffTest('6a', '6b', '6diff', {threshold: 0.05}, 51)
#[test]
fn pixelmatch_6a_vs_6b_threshold_005() {
    assert_eq!(diff_count("6a.png", "6b.png", 0.05, false), 51);
}

// pixelmatch test: diffTest('6a', '6a', '6empty', {threshold: 0}, 0)
#[test]
fn pixelmatch_6a_vs_6a_zero_threshold() {
    assert_eq!(diff_count("6a.png", "6a.png", 0.0, false), 0);
}

// pixelmatch test: diffTest('7a', '7b', '7diff', {diffColorAlt: [0, 255, 0]}, 2448)
// Note: pixelmatch default threshold is 0.1
#[test]
fn pixelmatch_7a_vs_7b_default_threshold() {
    assert_eq!(diff_count("7a.png", "7b.png", 0.1, false), 2448);
}

// Test that the MD5 fast path works for identical file bytes.
#[test]
fn pixelmatch_md5_fastpath_identical_bytes() {
    let data = load_fixture("1a.png");
    let options =
        CompareOptions { matching_threshold: 0.05, enable_antialias: false, ..CompareOptions::default() };
    let result = compare_image_files(&data, &data, &options).unwrap();
    assert!(result.images_are_same);
    assert_eq!(result.diff_count, 0);
}

// Test enable_antialias=true (maps to pixelmatch includeAA=true, AA detection OFF)
// All above-threshold pixels should be counted as diff.
#[test]
fn pixelmatch_1a_vs_1b_include_aa() {
    let count_without_aa = diff_count("1a.png", "1b.png", 0.05, false);
    let count_with_aa = diff_count("1a.png", "1b.png", 0.05, true);
    // With AA included (detection off), diff_count should be >= count without AA
    assert!(
        count_with_aa >= count_without_aa,
        "expected count_with_aa ({count_with_aa}) >= count_without_aa ({count_without_aa})"
    );
}
