use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

fn make_2x2_rgba(r: u8, g: u8, b: u8, a: u8) -> Vec<u8> {
    vec![r, g, b, a, r, g, b, a, r, g, b, a, r, g, b, a]
}

fn make_solid_rgba(width: u32, height: u32, r: u8, g: u8, b: u8, a: u8) -> Vec<u8> {
    let pixel_count = (width * height) as usize;
    let mut data = Vec::with_capacity(pixel_count * 4);
    for _ in 0..pixel_count {
        data.extend_from_slice(&[r, g, b, a]);
    }
    data
}

fn get_field(val: &JsValue, field: &str) -> JsValue {
    js_sys::Reflect::get(val, &JsValue::from_str(field)).unwrap()
}

fn get_regions(val: &JsValue) -> js_sys::Array {
    js_sys::Array::from(&get_field(val, "regions"))
}

#[wasm_bindgen_test]
fn test_compare_identical_pixels() {
    let img = make_2x2_rgba(255, 0, 0, 255);
    let result = kaiki_diff_wasm::compare_pixels(&img, &img, 2, 2, 0.0);

    let diff_count = get_field(&result, "diffCount").as_f64().unwrap() as u64;
    let total_pixels = get_field(&result, "totalPixels").as_f64().unwrap() as u64;
    let width = get_field(&result, "width").as_f64().unwrap() as u32;
    let height = get_field(&result, "height").as_f64().unwrap() as u32;

    assert_eq!(diff_count, 0);
    assert_eq!(total_pixels, 4);
    assert_eq!(width, 2);
    assert_eq!(height, 2);
}

#[wasm_bindgen_test]
fn test_compare_different_pixels() {
    let img_a = make_2x2_rgba(255, 0, 0, 255);
    let img_b = make_2x2_rgba(0, 255, 0, 255);
    let result = kaiki_diff_wasm::compare_pixels(&img_a, &img_b, 2, 2, 0.0);

    let diff_count = get_field(&result, "diffCount").as_f64().unwrap() as u64;
    let total_pixels = get_field(&result, "totalPixels").as_f64().unwrap() as u64;

    assert!(diff_count > 0);
    assert_eq!(total_pixels, 4);
}

#[wasm_bindgen_test]
fn test_result_has_camel_case_fields() {
    let img = make_2x2_rgba(128, 128, 128, 255);
    let result = kaiki_diff_wasm::compare_pixels(&img, &img, 2, 2, 0.1);

    assert!(!get_field(&result, "diffCount").is_undefined());
    assert!(!get_field(&result, "totalPixels").is_undefined());
    assert!(!get_field(&result, "width").is_undefined());
    assert!(!get_field(&result, "height").is_undefined());
}

#[wasm_bindgen_test]
fn test_threshold_affects_result() {
    let img_a = make_2x2_rgba(200, 0, 0, 255);
    let img_b = make_2x2_rgba(210, 0, 0, 255);

    let strict = kaiki_diff_wasm::compare_pixels(&img_a, &img_b, 2, 2, 0.0);
    let strict_diff = get_field(&strict, "diffCount").as_f64().unwrap() as u64;
    assert!(strict_diff > 0, "threshold=0.0 should detect differences");

    let lenient = kaiki_diff_wasm::compare_pixels(&img_a, &img_b, 2, 2, 1.0);
    let lenient_diff = get_field(&lenient, "diffCount").as_f64().unwrap() as u64;
    assert_eq!(lenient_diff, 0, "threshold=1.0 should treat small differences as identical");
}

#[wasm_bindgen_test]
fn test_transparent_pixels_treated_as_identical() {
    let img_a = make_2x2_rgba(255, 0, 0, 0);
    let img_b = make_2x2_rgba(0, 255, 0, 0);

    let result = kaiki_diff_wasm::compare_pixels(&img_a, &img_b, 2, 2, 0.0);
    let diff_count = get_field(&result, "diffCount").as_f64().unwrap() as u64;
    assert_eq!(diff_count, 0, "fully transparent pixels should be treated as identical");
}

#[wasm_bindgen_test]
fn test_single_pixel_comparison() {
    let a = vec![255u8, 0, 0, 255];
    let b = vec![0u8, 0, 255, 255];

    let result = kaiki_diff_wasm::compare_pixels(&a, &b, 1, 1, 0.0);

    let diff_count = get_field(&result, "diffCount").as_f64().unwrap() as u64;
    let total_pixels = get_field(&result, "totalPixels").as_f64().unwrap() as u64;
    let width = get_field(&result, "width").as_f64().unwrap() as u32;
    let height = get_field(&result, "height").as_f64().unwrap() as u32;

    assert_eq!(diff_count, 1);
    assert_eq!(total_pixels, 1);
    assert_eq!(width, 1);
    assert_eq!(height, 1);
}

#[wasm_bindgen_test]
fn test_regions_identical_images_no_regions() {
    let img = make_solid_rgba(4, 4, 100, 100, 100, 255);

    let result = kaiki_diff_wasm::compare_pixels_with_regions(&img, &img, 4, 4, 0.0, 1);

    let diff_count = get_field(&result, "diffCount").as_f64().unwrap() as u64;
    let regions = get_regions(&result);
    assert_eq!(diff_count, 0);
    assert_eq!(regions.length(), 0);
}

#[wasm_bindgen_test]
fn test_regions_all_different() {
    let img_a = make_solid_rgba(4, 4, 255, 0, 0, 255);
    let img_b = make_solid_rgba(4, 4, 0, 0, 255, 255);

    let result = kaiki_diff_wasm::compare_pixels_with_regions(&img_a, &img_b, 4, 4, 0.0, 1);

    let diff_count = get_field(&result, "diffCount").as_f64().unwrap() as u64;
    let regions = get_regions(&result);

    assert_eq!(diff_count, 16);
    assert_eq!(regions.length(), 1, "all-different 4x4 should produce a single region");

    let region = regions.get(0);
    let rx = get_field(&region, "x").as_f64().unwrap() as u32;
    let ry = get_field(&region, "y").as_f64().unwrap() as u32;
    let rw = get_field(&region, "width").as_f64().unwrap() as u32;
    let rh = get_field(&region, "height").as_f64().unwrap() as u32;
    assert_eq!((rx, ry, rw, rh), (0, 0, 4, 4));
}

#[wasm_bindgen_test]
fn test_regions_single_diff_pixel() {
    let mut img_a = make_solid_rgba(4, 4, 0, 0, 0, 255);
    let img_b = img_a.clone();
    let offset = 5 * 4; // pixel (1,1)
    img_a[offset] = 255;
    img_a[offset + 1] = 255;
    img_a[offset + 2] = 255;

    let result = kaiki_diff_wasm::compare_pixels_with_regions(&img_a, &img_b, 4, 4, 0.0, 1);

    let regions = get_regions(&result);
    assert_eq!(regions.length(), 1);

    let region = regions.get(0);
    let rw = get_field(&region, "width").as_f64().unwrap() as u32;
    let rh = get_field(&region, "height").as_f64().unwrap() as u32;
    assert_eq!(rw, 1);
    assert_eq!(rh, 1);
}

#[wasm_bindgen_test]
fn test_regions_has_camel_case_fields() {
    let img_a = make_solid_rgba(2, 2, 255, 0, 0, 255);
    let img_b = make_solid_rgba(2, 2, 0, 0, 255, 255);

    let result = kaiki_diff_wasm::compare_pixels_with_regions(&img_a, &img_b, 2, 2, 0.0, 1);

    assert!(!get_field(&result, "diffCount").is_undefined());
    assert!(!get_field(&result, "totalPixels").is_undefined());
    assert!(!get_field(&result, "regions").is_undefined());

    let regions = get_regions(&result);
    assert!(regions.length() > 0);
    let region = regions.get(0);
    assert!(!get_field(&region, "x").is_undefined());
    assert!(!get_field(&region, "y").is_undefined());
    assert!(!get_field(&region, "width").is_undefined());
    assert!(!get_field(&region, "height").is_undefined());
}

#[wasm_bindgen_test]
fn test_regions_min_area_filtering() {
    let mut img_a = make_solid_rgba(4, 4, 0, 0, 0, 255);
    let img_b = img_a.clone();
    img_a[0] = 255;
    img_a[1] = 255;
    img_a[2] = 255;

    let result1 = kaiki_diff_wasm::compare_pixels_with_regions(&img_a, &img_b, 4, 4, 0.0, 1);
    let regions1 = get_regions(&result1);
    assert_eq!(regions1.length(), 1, "min_area=1 should include the single-pixel region");

    let result2 = kaiki_diff_wasm::compare_pixels_with_regions(&img_a, &img_b, 4, 4, 0.0, 2);
    let regions2 = get_regions(&result2);
    assert_eq!(regions2.length(), 0, "min_area=2 should filter out a single-pixel region");
}

#[wasm_bindgen_test]
fn test_regions_two_separate_clusters() {
    let mut img_a = make_solid_rgba(6, 1, 0, 0, 0, 255);
    let img_b = img_a.clone();

    img_a[0] = 255; // pixel 0
    img_a[1] = 255;
    img_a[2] = 255;

    img_a[20] = 255; // pixel 5
    img_a[21] = 255;
    img_a[22] = 255;

    let result = kaiki_diff_wasm::compare_pixels_with_regions(&img_a, &img_b, 6, 1, 0.0, 1);

    let diff_count = get_field(&result, "diffCount").as_f64().unwrap() as u64;
    let regions = get_regions(&result);

    assert_eq!(diff_count, 2);
    assert_eq!(regions.length(), 2, "two separated diff pixels should form two regions");
}

#[wasm_bindgen_test]
fn test_regions_result_includes_diff_count() {
    let img_a = make_solid_rgba(4, 4, 255, 0, 0, 255);
    let img_b = make_solid_rgba(4, 4, 0, 0, 255, 255);

    let result = kaiki_diff_wasm::compare_pixels_with_regions(&img_a, &img_b, 4, 4, 0.0, 1);

    let diff_count = get_field(&result, "diffCount").as_f64().unwrap() as u64;
    let total_pixels = get_field(&result, "totalPixels").as_f64().unwrap() as u64;
    let regions = get_regions(&result);

    assert!(diff_count > 0, "diff_count should be positive");
    assert_eq!(total_pixels, 16);
    assert!(regions.length() > 0, "regions should be non-empty when there are diffs");
}
