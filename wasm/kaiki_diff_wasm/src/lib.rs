use wasm_bindgen::prelude::*;

/// Compare two RGBA pixel buffers and return the diff result.
#[wasm_bindgen]
pub fn compare_pixels(
    actual: &[u8],
    expected: &[u8],
    width: u32,
    height: u32,
    threshold: f64,
) -> JsValue {
    let actual_data = kaiki_diff::ImageData { width, height, data: actual.to_vec() };
    let expected_data = kaiki_diff::ImageData { width, height, data: expected.to_vec() };
    let options = kaiki_diff::CompareOptions {
        matching_threshold: threshold,
        enable_antialias: false,
        ..kaiki_diff::CompareOptions::default()
    };

    let result = kaiki_diff::compare_images(&actual_data, &expected_data, &options);

    serde_wasm_bindgen::to_value(&DiffResultJs {
        diff_count: result.diff_count,
        total_pixels: result.total_pixels,
        width: result.width,
        height: result.height,
    })
    .unwrap_or(JsValue::NULL)
}

/// Compare two RGBA pixel buffers and return the diff result with bounding box regions.
#[wasm_bindgen]
pub fn compare_pixels_with_regions(
    actual: &[u8],
    expected: &[u8],
    width: u32,
    height: u32,
    threshold: f64,
    min_region_area: u32,
) -> JsValue {
    let actual_data = kaiki_diff::ImageData { width, height, data: actual.to_vec() };
    let expected_data = kaiki_diff::ImageData { width, height, data: expected.to_vec() };
    let options = kaiki_diff::CompareOptions {
        matching_threshold: threshold,
        enable_antialias: false,
        ..kaiki_diff::CompareOptions::default()
    };

    let result = kaiki_diff::compare_images(&actual_data, &expected_data, &options);

    let regions = result
        .diff_mask
        .as_deref()
        .map(|mask| kaiki_diff::regions::detect_diff_regions(width, height, mask, min_region_area))
        .unwrap_or_default();

    serde_wasm_bindgen::to_value(&DiffResultWithRegionsJs {
        diff_count: result.diff_count,
        total_pixels: result.total_pixels,
        width: result.width,
        height: result.height,
        regions,
    })
    .unwrap_or(JsValue::NULL)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct DiffResultJs {
    diff_count: u64,
    total_pixels: u64,
    width: u32,
    height: u32,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct DiffResultWithRegionsJs {
    diff_count: u64,
    total_pixels: u64,
    width: u32,
    height: u32,
    regions: Vec<kaiki_diff::BoundingBox>,
}
