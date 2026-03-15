mod common;

use std::sync::Arc;

use kaiki_git::SimpleKeygen;

use common::{
    make_pipeline_processor, make_solid_png, MockNotifier, MockStorage, SharedMockNotifier,
    SharedMockStorage,
};

// ---------------------------------------------------------------------------
// Group A: run() full pipeline
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_run_full_pipeline() {
    let tmp = tempfile::tempdir().unwrap();
    let actual_dir = tmp.path().join("actual");
    let working_dir = tmp.path().join("working");
    std::fs::create_dir_all(&actual_dir).unwrap();

    let png = make_solid_png(2, 2, [255, 0, 0, 255]);
    std::fs::write(actual_dir.join("a.png"), &png).unwrap();

    let storage = Arc::new(MockStorage::new(
        vec![("a.png".to_string(), png.clone())],
        Some("https://example.com/report".to_string()),
    ));
    let notifier = Arc::new(MockNotifier::new());

    let keygen = SimpleKeygen { expected_key: "abc123".to_string() };
    let processor = make_pipeline_processor(
        &actual_dir,
        working_dir,
        Box::new(keygen),
        Some(Box::new(SharedMockStorage(Arc::clone(&storage)))),
        vec![Box::new(SharedMockNotifier(Arc::clone(&notifier)))],
    );

    let result = processor.run().await.unwrap();

    assert!(!result.has_failures);
    assert_eq!(result.report_url, Some("https://example.com/report".to_string()));
    assert_eq!(storage.fetch_calls.lock().unwrap().len(), 1);
    assert_eq!(storage.publish_calls.lock().unwrap().len(), 1);
    assert_eq!(notifier.calls.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn test_run_no_expected_key() {
    let tmp = tempfile::tempdir().unwrap();
    let actual_dir = tmp.path().join("actual");
    let working_dir = tmp.path().join("working");
    std::fs::create_dir_all(&actual_dir).unwrap();

    let png = make_solid_png(2, 2, [0, 255, 0, 255]);
    std::fs::write(actual_dir.join("a.png"), &png).unwrap();

    // empty expected_key → get_expected_key() returns None → sync skipped
    let storage = Arc::new(MockStorage::new(vec![], None));
    let notifier = Arc::new(MockNotifier::new());

    let keygen = SimpleKeygen { expected_key: String::new() };
    let processor = make_pipeline_processor(
        &actual_dir,
        working_dir,
        Box::new(keygen),
        Some(Box::new(SharedMockStorage(Arc::clone(&storage)))),
        vec![Box::new(SharedMockNotifier(Arc::clone(&notifier)))],
    );

    let result = processor.run().await.unwrap();

    // fetch should not have been called (no expected key)
    assert!(storage.fetch_calls.lock().unwrap().is_empty());
    // all images are new
    assert_eq!(result.comparison.new_items.len(), 1);
    assert!(result.comparison.new_items.iter().any(|n| n == "a.png"));
}

#[tokio::test]
async fn test_run_without_storage() {
    let tmp = tempfile::tempdir().unwrap();
    let actual_dir = tmp.path().join("actual");
    let working_dir = tmp.path().join("working");
    std::fs::create_dir_all(&actual_dir).unwrap();

    let png = make_solid_png(2, 2, [0, 0, 255, 255]);
    std::fs::write(actual_dir.join("a.png"), &png).unwrap();

    let notifier = Arc::new(MockNotifier::new());

    let keygen = SimpleKeygen { expected_key: "key1".to_string() };
    let processor = make_pipeline_processor(
        &actual_dir,
        working_dir,
        Box::new(keygen),
        None, // no storage
        vec![Box::new(SharedMockNotifier(Arc::clone(&notifier)))],
    );

    let result = processor.run().await.unwrap();

    // No storage → no report URL
    assert!(result.report_url.is_none());
    // compare still runs; a.png is new (no expected)
    assert_eq!(result.comparison.new_items.len(), 1);
    // notifier still called
    assert_eq!(notifier.calls.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn test_run_without_notifiers() {
    let tmp = tempfile::tempdir().unwrap();
    let actual_dir = tmp.path().join("actual");
    let working_dir = tmp.path().join("working");
    std::fs::create_dir_all(&actual_dir).unwrap();

    let png = make_solid_png(2, 2, [255, 255, 0, 255]);
    std::fs::write(actual_dir.join("a.png"), &png).unwrap();

    let storage = Arc::new(MockStorage::new(
        vec![("a.png".to_string(), png.clone())],
        Some("https://example.com".to_string()),
    ));

    let keygen = SimpleKeygen { expected_key: "key2".to_string() };
    let processor = make_pipeline_processor(
        &actual_dir,
        working_dir,
        Box::new(keygen),
        Some(Box::new(SharedMockStorage(Arc::clone(&storage)))),
        vec![], // no notifiers
    );

    let result = processor.run().await.unwrap();

    assert!(!result.has_failures);
    assert_eq!(result.report_url, Some("https://example.com".to_string()));
}

#[tokio::test]
async fn test_run_with_failures() {
    let tmp = tempfile::tempdir().unwrap();
    let actual_dir = tmp.path().join("actual");
    let working_dir = tmp.path().join("working");
    std::fs::create_dir_all(&actual_dir).unwrap();

    let original = make_solid_png(2, 2, [255, 0, 0, 255]);
    let different = make_solid_png(2, 2, [0, 255, 0, 255]);
    std::fs::write(actual_dir.join("a.png"), &different).unwrap();

    let storage = Arc::new(MockStorage::new(
        vec![("a.png".to_string(), original)],
        Some("https://example.com/diff".to_string()),
    ));
    let notifier = Arc::new(MockNotifier::new());

    let keygen = SimpleKeygen { expected_key: "key3".to_string() };
    let processor = make_pipeline_processor(
        &actual_dir,
        working_dir,
        Box::new(keygen),
        Some(Box::new(SharedMockStorage(Arc::clone(&storage)))),
        vec![Box::new(SharedMockNotifier(Arc::clone(&notifier)))],
    );

    let result = processor.run().await.unwrap();

    assert!(result.has_failures);
    assert!(!result.comparison.failed_items.is_empty());

    // notifier should have been called with has_failures info
    let calls = notifier.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert!(calls[0].comparison.has_failures());
}

// ---------------------------------------------------------------------------
// Group B: sync_expected()
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_sync_expected_populates_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let actual_dir = tmp.path().join("actual");
    let working_dir = tmp.path().join("working");
    std::fs::create_dir_all(&actual_dir).unwrap();

    let img1 = make_solid_png(2, 2, [255, 0, 0, 255]);
    let img2 = make_solid_png(2, 2, [0, 255, 0, 255]);

    let storage = Arc::new(MockStorage::new(
        vec![("a.png".to_string(), img1), ("b.png".to_string(), img2)],
        None,
    ));

    let keygen = SimpleKeygen { expected_key: "sync-key".to_string() };
    let processor = make_pipeline_processor(
        &actual_dir,
        working_dir.clone(),
        Box::new(keygen),
        Some(Box::new(SharedMockStorage(Arc::clone(&storage)))),
        vec![],
    );

    processor.sync_expected("sync-key").await.unwrap();

    let expected_dir = working_dir.join("expected");
    assert!(expected_dir.join("a.png").exists());
    assert!(expected_dir.join("b.png").exists());
    assert_eq!(*storage.fetch_calls.lock().unwrap(), vec!["sync-key"]);
}

#[tokio::test]
async fn test_sync_expected_nested_paths() {
    let tmp = tempfile::tempdir().unwrap();
    let actual_dir = tmp.path().join("actual");
    let working_dir = tmp.path().join("working");
    std::fs::create_dir_all(&actual_dir).unwrap();

    let img = make_solid_png(2, 2, [128, 128, 128, 255]);

    let storage = Arc::new(MockStorage::new(
        vec![("sub/dir/a.png".to_string(), img)],
        None,
    ));

    let keygen = SimpleKeygen { expected_key: "nested-key".to_string() };
    let processor = make_pipeline_processor(
        &actual_dir,
        working_dir.clone(),
        Box::new(keygen),
        Some(Box::new(SharedMockStorage(Arc::clone(&storage)))),
        vec![],
    );

    processor.sync_expected("nested-key").await.unwrap();

    assert!(working_dir.join("expected/sub/dir/a.png").exists());
}

#[tokio::test]
async fn test_sync_expected_no_storage_noop() {
    let tmp = tempfile::tempdir().unwrap();
    let actual_dir = tmp.path().join("actual");
    let working_dir = tmp.path().join("working");
    std::fs::create_dir_all(&actual_dir).unwrap();

    let keygen = SimpleKeygen { expected_key: "key".to_string() };
    let processor = make_pipeline_processor(
        &actual_dir,
        working_dir.clone(),
        Box::new(keygen),
        None, // no storage
        vec![],
    );

    processor.sync_expected("key").await.unwrap();

    let expected_dir = working_dir.join("expected");
    // Directory is created but empty
    assert!(expected_dir.exists());
    assert_eq!(std::fs::read_dir(&expected_dir).unwrap().count(), 0);
}

// ---------------------------------------------------------------------------
// Group C: publish() / notify()
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_publish_returns_report_url() {
    let tmp = tempfile::tempdir().unwrap();
    let actual_dir = tmp.path().join("actual");
    let working_dir = tmp.path().join("working");
    std::fs::create_dir_all(&actual_dir).unwrap();

    let storage = Arc::new(MockStorage::new(
        vec![],
        Some("https://reports.example.com/123".to_string()),
    ));

    let keygen = SimpleKeygen { expected_key: "pub-key".to_string() };
    let processor = make_pipeline_processor(
        &actual_dir,
        working_dir,
        Box::new(keygen),
        Some(Box::new(SharedMockStorage(Arc::clone(&storage)))),
        vec![],
    );

    let url = processor.publish("pub-key").await.unwrap();

    assert_eq!(url, Some("https://reports.example.com/123".to_string()));
    assert_eq!(*storage.publish_calls.lock().unwrap(), vec!["pub-key"]);
}

#[tokio::test]
async fn test_notify_calls_all_notifiers() {
    let tmp = tempfile::tempdir().unwrap();
    let actual_dir = tmp.path().join("actual");
    let working_dir = tmp.path().join("working");
    std::fs::create_dir_all(&actual_dir).unwrap();

    let notifier1 = Arc::new(MockNotifier::new());
    let notifier2 = Arc::new(MockNotifier::new());

    let keygen = SimpleKeygen { expected_key: "key".to_string() };
    let processor = make_pipeline_processor(
        &actual_dir,
        working_dir,
        Box::new(keygen),
        None,
        vec![
            Box::new(SharedMockNotifier(Arc::clone(&notifier1))),
            Box::new(SharedMockNotifier(Arc::clone(&notifier2))),
        ],
    );

    let params = kaiki_notify::NotifyParams {
        comparison: kaiki_report::ComparisonResult {
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
        },
        report_url: None,
        current_sha: "abc".to_string(),
        pr_number: None,
    };

    processor.notify(&params).await.unwrap();

    assert_eq!(notifier1.calls.lock().unwrap().len(), 1);
    assert_eq!(notifier2.calls.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn test_notify_swallows_failure() {
    let tmp = tempfile::tempdir().unwrap();
    let actual_dir = tmp.path().join("actual");
    let working_dir = tmp.path().join("working");
    std::fs::create_dir_all(&actual_dir).unwrap();

    let png = make_solid_png(2, 2, [255, 0, 0, 255]);
    std::fs::write(actual_dir.join("a.png"), &png).unwrap();

    let storage = Arc::new(MockStorage::new(
        vec![("a.png".to_string(), png)],
        Some("https://example.com".to_string()),
    ));
    let notifier = Arc::new(MockNotifier::failing("webhook down"));

    let keygen = SimpleKeygen { expected_key: "key".to_string() };
    let processor = make_pipeline_processor(
        &actual_dir,
        working_dir,
        Box::new(keygen),
        Some(Box::new(SharedMockStorage(Arc::clone(&storage)))),
        vec![Box::new(SharedMockNotifier(Arc::clone(&notifier)))],
    );

    // run() should succeed even though notifier fails
    let result = processor.run().await.unwrap();

    assert!(!result.has_failures);
    // notifier was still called (it just failed)
    assert_eq!(notifier.calls.lock().unwrap().len(), 1);
}

// ---------------------------------------------------------------------------
// Group D: PipelineResult verification
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_result_has_failures_reflects_comparison() {
    let tmp = tempfile::tempdir().unwrap();
    let actual_dir = tmp.path().join("actual");
    let working_dir = tmp.path().join("working");
    std::fs::create_dir_all(&actual_dir).unwrap();

    // Identical images → no failures
    let png = make_solid_png(4, 4, [100, 100, 100, 255]);
    std::fs::write(actual_dir.join("same.png"), &png).unwrap();

    let storage = Arc::new(MockStorage::new(
        vec![("same.png".to_string(), png.clone())],
        None,
    ));
    let keygen = SimpleKeygen { expected_key: "key-same".to_string() };
    let processor = make_pipeline_processor(
        &actual_dir,
        working_dir,
        Box::new(keygen),
        Some(Box::new(SharedMockStorage(Arc::clone(&storage)))),
        vec![],
    );

    let result = processor.run().await.unwrap();
    assert!(!result.has_failures);
    assert!(result.comparison.failed_items.is_empty());
    assert!(!result.comparison.passed_items.is_empty());

    // Different images → has failures
    let tmp2 = tempfile::tempdir().unwrap();
    let actual_dir2 = tmp2.path().join("actual");
    let working_dir2 = tmp2.path().join("working");
    std::fs::create_dir_all(&actual_dir2).unwrap();

    let different = make_solid_png(4, 4, [200, 200, 200, 255]);
    std::fs::write(actual_dir2.join("diff.png"), &different).unwrap();

    let storage2 = Arc::new(MockStorage::new(
        vec![("diff.png".to_string(), png)],
        None,
    ));
    let keygen2 = SimpleKeygen { expected_key: "key-diff".to_string() };
    let processor2 = make_pipeline_processor(
        &actual_dir2,
        working_dir2,
        Box::new(keygen2),
        Some(Box::new(SharedMockStorage(Arc::clone(&storage2)))),
        vec![],
    );

    let result2 = processor2.run().await.unwrap();
    assert!(result2.has_failures);
    assert!(!result2.comparison.failed_items.is_empty());
}

#[tokio::test]
async fn test_result_report_url_from_storage() {
    let tmp = tempfile::tempdir().unwrap();
    let actual_dir = tmp.path().join("actual");
    let working_dir = tmp.path().join("working");
    std::fs::create_dir_all(&actual_dir).unwrap();

    let png = make_solid_png(2, 2, [50, 50, 50, 255]);
    std::fs::write(actual_dir.join("x.png"), &png).unwrap();

    let url = "https://my-bucket.s3.amazonaws.com/report/index.html";
    let storage = Arc::new(MockStorage::new(
        vec![("x.png".to_string(), png)],
        Some(url.to_string()),
    ));

    let keygen = SimpleKeygen { expected_key: "url-key".to_string() };
    let processor = make_pipeline_processor(
        &actual_dir,
        working_dir,
        Box::new(keygen),
        Some(Box::new(SharedMockStorage(Arc::clone(&storage)))),
        vec![],
    );

    let result = processor.run().await.unwrap();

    assert_eq!(result.report_url, Some(url.to_string()));
}
