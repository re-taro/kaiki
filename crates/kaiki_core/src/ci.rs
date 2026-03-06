/// Detect pull request number from CI environment variables.
///
/// Detection sources (in priority order):
/// 1. `REG_SUIT_PR_NUMBER` — direct env var override
/// 2. `GITHUB_EVENT_PATH` — GitHub Actions event JSON (`pull_request.number` or `issue.number`)
/// 3. `GITHUB_REF` — parse `refs/pull/{number}/merge`
pub fn detect_pr_number() -> Option<u64> {
    // 1. Direct env var
    if let Ok(val) = std::env::var("REG_SUIT_PR_NUMBER")
        && let Ok(pr) = val.parse()
    {
        return Some(pr);
    }

    // 2. GITHUB_EVENT_PATH
    if let Ok(path) = std::env::var("GITHUB_EVENT_PATH")
        && let Ok(content) = std::fs::read_to_string(&path)
        && let Ok(event) = serde_json::from_str::<serde_json::Value>(&content)
        && let Some(pr) = event["pull_request"]["number"]
            .as_u64()
            .or_else(|| event["issue"]["number"].as_u64())
    {
        return Some(pr);
    }

    // 3. GITHUB_REF
    if let Ok(gh_ref) = std::env::var("GITHUB_REF") {
        // refs/pull/123/merge
        let parts: Vec<&str> = gh_ref.split('/').collect();
        if parts.len() >= 3 && parts[1] == "pull" {
            return parts[2].parse().ok();
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Mutex to serialize tests that mutate environment variables
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Clear all CI-related env vars to avoid cross-test contamination.
    ///
    /// # Safety
    /// Must be called only in tests with `ENV_LOCK` held. Mutates process-wide env vars.
    unsafe fn clear_ci_env() {
        // SAFETY: Called under ENV_LOCK, single-threaded access to env vars guaranteed.
        unsafe {
            std::env::remove_var("REG_SUIT_PR_NUMBER");
            std::env::remove_var("GITHUB_EVENT_PATH");
            std::env::remove_var("GITHUB_REF");
        }
    }

    #[test]
    fn test_detect_from_reg_suit_env() {
        let _lock = ENV_LOCK.lock().unwrap();
        // SAFETY: env var mutation serialized by ENV_LOCK
        unsafe { clear_ci_env() };
        // SAFETY: env var mutation serialized by ENV_LOCK
        unsafe { std::env::set_var("REG_SUIT_PR_NUMBER", "42") };
        let result = detect_pr_number();
        // SAFETY: env var mutation serialized by ENV_LOCK
        unsafe { clear_ci_env() };
        assert_eq!(result, Some(42));
    }

    #[test]
    fn test_detect_from_github_ref() {
        let _lock = ENV_LOCK.lock().unwrap();
        // SAFETY: env var mutation serialized by ENV_LOCK
        unsafe { clear_ci_env() };
        // SAFETY: env var mutation serialized by ENV_LOCK
        unsafe { std::env::set_var("GITHUB_REF", "refs/pull/123/merge") };
        let result = detect_pr_number();
        // SAFETY: env var mutation serialized by ENV_LOCK
        unsafe { clear_ci_env() };
        assert_eq!(result, Some(123));
    }

    #[test]
    fn test_detect_from_github_event_path() {
        let _lock = ENV_LOCK.lock().unwrap();
        let dir = std::env::temp_dir();
        let event_path = dir.join("test_gh_event.json");
        std::fs::write(&event_path, r#"{"pull_request": {"number": 456}}"#).unwrap();

        // SAFETY: env var mutation serialized by ENV_LOCK
        unsafe { clear_ci_env() };
        // SAFETY: env var mutation serialized by ENV_LOCK
        unsafe { std::env::set_var("GITHUB_EVENT_PATH", event_path.to_str().unwrap()) };
        let result = detect_pr_number();
        // SAFETY: env var mutation serialized by ENV_LOCK
        unsafe { clear_ci_env() };
        let _ = std::fs::remove_file(&event_path);
        assert_eq!(result, Some(456));
    }

    #[test]
    fn test_detect_none_when_no_env() {
        let _lock = ENV_LOCK.lock().unwrap();
        // SAFETY: env var mutation serialized by ENV_LOCK
        unsafe { clear_ci_env() };
        let result = detect_pr_number();
        assert_eq!(result, None);
    }

    #[test]
    fn test_detect_invalid_pr_number_falls_through() {
        let _lock = ENV_LOCK.lock().unwrap();
        // SAFETY: env var mutation serialized by ENV_LOCK
        unsafe { clear_ci_env() };
        // SAFETY: env var mutation serialized by ENV_LOCK
        unsafe { std::env::set_var("REG_SUIT_PR_NUMBER", "not_a_number") };
        // SAFETY: env var mutation serialized by ENV_LOCK
        unsafe { std::env::set_var("GITHUB_REF", "refs/pull/789/merge") };
        let result = detect_pr_number();
        // SAFETY: env var mutation serialized by ENV_LOCK
        unsafe { clear_ci_env() };
        assert_eq!(result, Some(789));
    }
}
