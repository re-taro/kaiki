use askama::Template;
pub use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ReportError {
    #[error("failed to write report: {0}")]
    Io(#[from] std::io::Error),
    #[error("template rendering failed: {0}")]
    Template(String),
    #[error("serialization failed: {0}")]
    Serialize(#[from] serde_json::Error),
}

/// The comparison result written to out.json (reg-suit compatible).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComparisonResult {
    pub failed_items: Vec<CompactString>,
    pub new_items: Vec<CompactString>,
    pub deleted_items: Vec<CompactString>,
    pub passed_items: Vec<CompactString>,
    pub expected_items: Vec<CompactString>,
    pub actual_items: Vec<CompactString>,
    pub diff_items: Vec<CompactString>,
    pub actual_dir: CompactString,
    pub expected_dir: CompactString,
    pub diff_dir: CompactString,
}

impl ComparisonResult {
    /// Check if the comparison has any failures.
    pub fn has_failures(&self) -> bool {
        !self.failed_items.is_empty()
    }

    /// Check if there are any changes (new, deleted, or failed items).
    pub fn has_changes(&self) -> bool {
        !self.failed_items.is_empty()
            || !self.new_items.is_empty()
            || !self.deleted_items.is_empty()
    }
}

/// Determine if an image comparison passes the threshold.
pub fn is_passed(
    diff_count: u64,
    total_pixels: u64,
    threshold_pixel: Option<u64>,
    threshold_rate: Option<f64>,
) -> bool {
    if let Some(tp) = threshold_pixel {
        return diff_count <= tp;
    }
    if let Some(tr) = threshold_rate {
        if total_pixels == 0 {
            return true;
        }
        return (diff_count as f64 / total_pixels as f64) <= tr;
    }
    diff_count == 0
}

/// Write the comparison result as out.json.
pub fn write_json_report(
    result: &ComparisonResult,
    output_path: &std::path::Path,
) -> Result<(), ReportError> {
    let json = serde_json::to_string_pretty(result)?;
    std::fs::write(output_path, json)?;
    Ok(())
}

/// Generate and write the HTML report.
pub fn write_html_report(
    result: &ComparisonResult,
    output_path: &std::path::Path,
    ximgdiff_enabled: bool,
) -> Result<(), ReportError> {
    let json_data = serde_json::to_string(result)?;
    let has_failures = result.has_failures();
    let html = render_html_report(&json_data, has_failures, ximgdiff_enabled)?;
    std::fs::write(output_path, html)?;
    Ok(())
}

#[derive(Template)]
#[template(path = "report.html")]
struct ReportTemplate<'a> {
    favicon: &'a str,
    css: &'a str,
    js: &'a str,
    json_data: &'a str,
    ximgdiff_enabled: bool,
}

fn render_html_report(
    json_data: &str,
    has_failures: bool,
    ximgdiff_enabled: bool,
) -> Result<String, ReportError> {
    let favicon = if has_failures { FAVICON_FAILURE } else { FAVICON_SUCCESS };

    let template = ReportTemplate { favicon, css: CSS, js: JS, json_data, ximgdiff_enabled };

    template.render().map_err(|e| ReportError::Template(e.to_string()))
}

#[cfg(feature = "ximgdiff")]
const XIMGDIFF_WASM: &[u8] =
    include_bytes!("../../../wasm/kaiki_diff_wasm/pkg/kaiki_diff_wasm_bg.wasm");
#[cfg(feature = "ximgdiff")]
const XIMGDIFF_JS: &str =
    include_str!("../../../wasm/kaiki_diff_wasm/pkg/kaiki_diff_wasm.js");

/// Write ximgdiff wasm assets to the output directory.
#[cfg(feature = "ximgdiff")]
pub fn write_ximgdiff_assets(output_dir: &std::path::Path) -> Result<(), ReportError> {
    std::fs::write(output_dir.join("kaiki_diff_wasm_bg.wasm"), XIMGDIFF_WASM)?;
    std::fs::write(output_dir.join("kaiki_diff_wasm.js"), XIMGDIFF_JS)?;
    Ok(())
}

/// No-op when ximgdiff feature is not enabled.
#[cfg(not(feature = "ximgdiff"))]
pub fn write_ximgdiff_assets(_output_dir: &std::path::Path) -> Result<(), ReportError> {
    Ok(())
}

const CSS: &str = include_str!("report.css");
const JS: &str = include_str!("report.js");

const FAVICON_SUCCESS: &str = "data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 100 100'><text y='.9em' font-size='90'>✅</text></svg>";
const FAVICON_FAILURE: &str = "data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 100 100'><text y='.9em' font-size='90'>❌</text></svg>";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_passed_zero_diff() {
        assert!(is_passed(0, 100, None, None));
    }

    #[test]
    fn test_is_passed_nonzero_diff_no_threshold() {
        assert!(!is_passed(1, 100, None, None));
    }

    #[test]
    fn test_is_passed_threshold_pixel() {
        assert!(is_passed(5, 100, Some(5), None));
        assert!(!is_passed(6, 100, Some(5), None));
    }

    #[test]
    fn test_is_passed_threshold_rate() {
        assert!(is_passed(10, 100, None, Some(0.1)));
        assert!(!is_passed(11, 100, None, Some(0.1)));
    }

    #[test]
    fn test_is_passed_pixel_takes_precedence() {
        // threshold_pixel should take precedence over threshold_rate
        assert!(is_passed(5, 100, Some(5), Some(0.01)));
    }

    fn sample_result() -> ComparisonResult {
        ComparisonResult {
            failed_items: vec!["a.png".into()],
            new_items: vec!["b.png".into()],
            deleted_items: vec![],
            passed_items: vec!["c.png".into()],
            expected_items: vec!["a.png".into(), "c.png".into()],
            actual_items: vec!["a.png".into(), "b.png".into(), "c.png".into()],
            diff_items: vec!["a.png".into(), "c.png".into()],
            actual_dir: "actual".into(),
            expected_dir: "expected".into(),
            diff_dir: "diff".into(),
        }
    }

    #[test]
    fn test_has_failures() {
        let result = sample_result();
        assert!(result.has_failures());

        let empty = ComparisonResult {
            failed_items: vec![],
            ..sample_result()
        };
        assert!(!empty.has_failures());
    }

    #[test]
    fn test_has_changes() {
        // Has failures → has changes
        assert!(sample_result().has_changes());

        // Only new items → has changes
        let result = ComparisonResult {
            failed_items: vec![],
            new_items: vec!["x.png".into()],
            deleted_items: vec![],
            ..sample_result()
        };
        assert!(result.has_changes());

        // Only deleted items → has changes
        let result = ComparisonResult {
            failed_items: vec![],
            new_items: vec![],
            deleted_items: vec!["y.png".into()],
            ..sample_result()
        };
        assert!(result.has_changes());

        // No changes at all
        let result = ComparisonResult {
            failed_items: vec![],
            new_items: vec![],
            deleted_items: vec![],
            ..sample_result()
        };
        assert!(!result.has_changes());
    }

    #[test]
    fn test_json_report_format() {
        let result = sample_result();
        let json = serde_json::to_string(&result).unwrap();
        // Should use camelCase
        assert!(json.contains("failedItems"));
        assert!(json.contains("newItems"));
        assert!(json.contains("deletedItems"));
        assert!(json.contains("passedItems"));
        assert!(json.contains("actualDir"));
        assert!(json.contains("expectedDir"));
        assert!(json.contains("diffDir"));
    }

    #[test]
    fn test_json_report_roundtrip() {
        let result = sample_result();
        let json = serde_json::to_string(&result).unwrap();
        let deserialized: ComparisonResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.failed_items, result.failed_items);
        assert_eq!(deserialized.new_items, result.new_items);
        assert_eq!(deserialized.passed_items, result.passed_items);
    }

    #[test]
    fn test_html_report_contains_data() {
        let json_data = r#"{"failedItems":["a.png"]}"#;
        let html = render_html_report(json_data, true, false).unwrap();
        assert!(html.contains(json_data));
        assert!(html.contains("reg-data"));
    }

    #[test]
    fn test_html_report_favicon() {
        let html_fail = render_html_report("{}", true, false).unwrap();
        assert!(html_fail.contains(FAVICON_FAILURE));

        let html_ok = render_html_report("{}", false, false).unwrap();
        assert!(html_ok.contains(FAVICON_SUCCESS));
    }

    #[test]
    fn test_html_report_includes_css_and_js() {
        let html = render_html_report("{}", false, false).unwrap();
        // CSS should contain our custom styles
        assert!(html.contains(".header"));
        assert!(html.contains(".slider-container"));
        // JS should contain our app code
        assert!(html.contains("reg-data"));
        assert!(html.contains("tab-panel"));
    }

    #[test]
    fn test_empty_comparison_result_report() {
        let empty = ComparisonResult {
            failed_items: vec![],
            new_items: vec![],
            deleted_items: vec![],
            passed_items: vec![],
            expected_items: vec![],
            actual_items: vec![],
            diff_items: vec![],
            actual_dir: "actual".into(),
            expected_dir: "expected".into(),
            diff_dir: "diff".into(),
        };

        // JSON serialization should succeed
        let json = serde_json::to_string_pretty(&empty).unwrap();
        assert!(json.contains("\"failedItems\""));

        // HTML report should render without error
        let html = render_html_report(&json, false, false).unwrap();
        assert!(!html.is_empty());
    }

    #[test]
    fn test_html_report_ximgdiff_enabled() {
        let html = render_html_report("{}", false, true).unwrap();
        assert!(html.contains("kaiki_diff_wasm.js"));
        assert!(html.contains("kaiki-wasm-ready"));
    }

    #[test]
    fn test_html_report_ximgdiff_disabled() {
        let html = render_html_report("{}", false, false).unwrap();
        assert!(!html.contains("kaiki_diff_wasm.js"));
    }
}
