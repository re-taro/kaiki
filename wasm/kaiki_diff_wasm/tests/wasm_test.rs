use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

/// Helper: create a solid 2x2 RGBA image (8 pixels = 16 bytes).
fn make_2x2_rgba(r: u8, g: u8, b: u8, a: u8) -> Vec<u8> {
    vec![r, g, b, a, r, g, b, a, r, g, b, a, r, g, b, a]
}

fn get_field(val: &JsValue, field: &str) -> JsValue {
    js_sys::Reflect::get(val, &JsValue::from_str(field)).unwrap()
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

    // Verify camelCase field names exist
    assert!(!get_field(&result, "diffCount").is_undefined());
    assert!(!get_field(&result, "totalPixels").is_undefined());
    assert!(!get_field(&result, "width").is_undefined());
    assert!(!get_field(&result, "height").is_undefined());
}
