use std::collections::HashMap;
use std::io::Write;
use std::sync::{Arc, Mutex};

use flate2::Compression;
use flate2::write::GzEncoder;
use kaiki_config::S3PluginConfig;
use kaiki_storage::Storage;
use kaiki_storage::s3::S3Storage;
use kaiki_storage::s3_client::{
    GetObjectOutput, ListObjectsOutput, ObjectEntry, PutObjectParams, S3Client,
};
use kaiki_storage::{StorageError, UPLOAD_EXTENSIONS};

// ---------------------------------------------------------------------------
// Mock S3 Client
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct MockS3Client {
    objects: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    list_responses: Arc<Mutex<Vec<ListObjectsOutput>>>,
    get_error: Arc<Mutex<Option<String>>>,
    put_calls: Arc<Mutex<Vec<PutCall>>>,
}

#[derive(Debug, Clone)]
struct PutCall {
    _bucket: String,
    key: String,
    body: Vec<u8>,
    params: PutObjectParams,
}

impl MockS3Client {
    fn new() -> Self {
        Self {
            objects: Arc::new(Mutex::new(HashMap::new())),
            list_responses: Arc::new(Mutex::new(Vec::new())),
            get_error: Arc::new(Mutex::new(None)),
            put_calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn add_object(&self, key: &str, data: Vec<u8>) {
        self.objects.lock().unwrap().insert(key.to_string(), data);
    }

    fn add_object_with_encoding(&self, key: &str, data: Vec<u8>, encoding: &str) {
        let full_key = format!("{key}::{encoding}");
        self.objects.lock().unwrap().insert(full_key, data);
    }

    fn set_list_responses(&self, responses: Vec<ListObjectsOutput>) {
        *self.list_responses.lock().unwrap() = responses;
    }

    fn set_get_error(&self, msg: &str) {
        *self.get_error.lock().unwrap() = Some(msg.to_string());
    }

    fn get_put_calls(&self) -> Vec<PutCall> {
        self.put_calls.lock().unwrap().clone()
    }
}

impl S3Client for MockS3Client {
    async fn list_objects_v2(
        &self,
        _bucket: &str,
        prefix: &str,
        _continuation_token: Option<&str>,
    ) -> Result<ListObjectsOutput, StorageError> {
        let mut responses = self.list_responses.lock().unwrap();
        if !responses.is_empty() {
            return Ok(responses.remove(0));
        }

        // Auto-generate from objects
        let objects = self.objects.lock().unwrap();
        let entries: Vec<ObjectEntry> = objects
            .keys()
            .filter(|k| {
                let actual_key = k.split("::").next().unwrap();
                actual_key.starts_with(prefix)
            })
            .map(|k| ObjectEntry { key: k.split("::").next().unwrap().to_string() })
            .collect();

        Ok(ListObjectsOutput {
            objects: entries,
            is_truncated: false,
            next_continuation_token: None,
        })
    }

    async fn get_object(&self, _bucket: &str, key: &str) -> Result<GetObjectOutput, StorageError> {
        if let Some(ref msg) = *self.get_error.lock().unwrap() {
            return Err(StorageError::S3(msg.clone().into()));
        }

        let objects = self.objects.lock().unwrap();

        // Check for encoding-tagged entry first
        for (stored_key, data) in objects.iter() {
            if stored_key.contains("::") {
                let parts: Vec<&str> = stored_key.splitn(2, "::").collect();
                if parts[0] == key {
                    return Ok(GetObjectOutput {
                        body: data.clone(),
                        content_encoding: Some(parts[1].to_string()),
                    });
                }
            }
        }

        // Plain entry
        if let Some(data) = objects.get(key) {
            return Ok(GetObjectOutput { body: data.clone(), content_encoding: None });
        }

        Err(StorageError::S3("object not found".into()))
    }

    async fn put_object(
        &self,
        bucket: &str,
        key: &str,
        body: Vec<u8>,
        params: PutObjectParams,
    ) -> Result<(), StorageError> {
        self.put_calls.lock().unwrap().push(PutCall {
            _bucket: bucket.to_string(),
            key: key.to_string(),
            body,
            params,
        });
        Ok(())
    }
}

fn default_s3_config() -> S3PluginConfig {
    S3PluginConfig {
        bucket_name: "test-bucket".to_string(),
        acl: None,
        sse: None,
        sse_kms_key_id: None,
        path_prefix: None,
        endpoint: None,
        region: None,
    }
}

fn gzip_compress(data: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

// ---------------------------------------------------------------------------
// Fetch tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_fetch_single_file() {
    let client = MockS3Client::new();
    client.add_object("abc123/screenshot.png", b"image-data".to_vec());

    let storage = S3Storage::with_client(default_s3_config(), client);
    let dest = tempfile::tempdir().unwrap();

    storage.fetch("abc123", dest.path()).await.unwrap();

    let content = std::fs::read(dest.path().join("screenshot.png")).unwrap();
    assert_eq!(content, b"image-data");
}

#[tokio::test]
async fn test_fetch_multiple_files() {
    let client = MockS3Client::new();
    client.add_object("key1/a.png", b"data-a".to_vec());
    client.add_object("key1/b.png", b"data-b".to_vec());
    client.add_object("key1/c.png", b"data-c".to_vec());

    let storage = S3Storage::with_client(default_s3_config(), client);
    let dest = tempfile::tempdir().unwrap();

    storage.fetch("key1", dest.path()).await.unwrap();

    assert_eq!(std::fs::read(dest.path().join("a.png")).unwrap(), b"data-a");
    assert_eq!(std::fs::read(dest.path().join("b.png")).unwrap(), b"data-b");
    assert_eq!(std::fs::read(dest.path().join("c.png")).unwrap(), b"data-c");
}

#[tokio::test]
async fn test_fetch_with_pagination() {
    let client = MockS3Client::new();
    client.add_object("key1/first.png", b"first".to_vec());
    client.add_object("key1/second.png", b"second".to_vec());

    client.set_list_responses(vec![
        ListObjectsOutput {
            objects: vec![ObjectEntry { key: "key1/first.png".to_string() }],
            is_truncated: true,
            next_continuation_token: Some("token-1".to_string()),
        },
        ListObjectsOutput {
            objects: vec![ObjectEntry { key: "key1/second.png".to_string() }],
            is_truncated: false,
            next_continuation_token: None,
        },
    ]);

    let storage = S3Storage::with_client(default_s3_config(), client);
    let dest = tempfile::tempdir().unwrap();

    storage.fetch("key1", dest.path()).await.unwrap();

    assert_eq!(std::fs::read(dest.path().join("first.png")).unwrap(), b"first");
    assert_eq!(std::fs::read(dest.path().join("second.png")).unwrap(), b"second");
}

#[tokio::test]
async fn test_fetch_gzip_decompression() {
    let original = b"hello compressed world";
    let compressed = gzip_compress(original);

    let client = MockS3Client::new();
    client.add_object_with_encoding("key1/file.png", compressed, "gzip");

    let storage = S3Storage::with_client(default_s3_config(), client);
    let dest = tempfile::tempdir().unwrap();

    storage.fetch("key1", dest.path()).await.unwrap();

    let content = std::fs::read(dest.path().join("file.png")).unwrap();
    assert_eq!(content, original);
}

#[tokio::test]
async fn test_fetch_nested_paths() {
    let client = MockS3Client::new();
    client.add_object("key1/sub/dir/file.png", b"nested-data".to_vec());

    let storage = S3Storage::with_client(default_s3_config(), client);
    let dest = tempfile::tempdir().unwrap();

    storage.fetch("key1", dest.path()).await.unwrap();

    let content = std::fs::read(dest.path().join("sub/dir/file.png")).unwrap();
    assert_eq!(content, b"nested-data");
}

#[tokio::test]
async fn test_fetch_empty_list() {
    let client = MockS3Client::new();
    client.set_list_responses(vec![ListObjectsOutput {
        objects: vec![],
        is_truncated: false,
        next_continuation_token: None,
    }]);

    let storage = S3Storage::with_client(default_s3_config(), client);
    let dest = tempfile::tempdir().unwrap();

    storage.fetch("key1", dest.path()).await.unwrap();

    // dest_dir should have no files
    let entries: Vec<_> = std::fs::read_dir(dest.path()).unwrap().collect();
    assert!(entries.is_empty());
}

#[tokio::test]
async fn test_fetch_error_propagation() {
    let client = MockS3Client::new();
    client.add_object("key1/file.png", b"data".to_vec());
    client.set_get_error("simulated failure");

    let storage = S3Storage::with_client(default_s3_config(), client);
    let dest = tempfile::tempdir().unwrap();

    let result = storage.fetch("key1", dest.path()).await;
    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("S3 error"), "unexpected error: {err_msg}");
}

// ---------------------------------------------------------------------------
// Publish tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_publish_single_file() {
    let client = MockS3Client::new();
    let source = tempfile::tempdir().unwrap();

    std::fs::write(source.path().join("screenshot.png"), b"image-data").unwrap();

    let storage = S3Storage::with_client(default_s3_config(), client.clone());

    let result = storage.publish("abc123", source.path()).await.unwrap();
    assert!(result.report_url.is_some());

    let calls = client.get_put_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].key, "abc123/screenshot.png");
    assert_eq!(calls[0].params.content_encoding, "gzip");

    // Verify body is gzip-compressed
    let decompressed = decompress_gzip(&calls[0].body);
    assert_eq!(decompressed, b"image-data");
}

#[tokio::test]
async fn test_publish_filters_extensions() {
    let client = MockS3Client::new();
    let source = tempfile::tempdir().unwrap();

    std::fs::write(source.path().join("image.png"), b"png-data").unwrap();
    std::fs::write(source.path().join("report.html"), b"html-data").unwrap();
    std::fs::write(source.path().join("notes.txt"), b"text-data").unwrap();
    std::fs::write(source.path().join("data.json"), b"json-data").unwrap();

    let storage = S3Storage::with_client(default_s3_config(), client.clone());
    storage.publish("key1", source.path()).await.unwrap();

    let calls = client.get_put_calls();
    let uploaded_keys: Vec<&str> = calls.iter().map(|c| c.key.as_str()).collect();

    // png, html, json should be uploaded; txt should not
    assert!(uploaded_keys.iter().any(|k| k.contains("image.png")));
    assert!(uploaded_keys.iter().any(|k| k.contains("report.html")));
    assert!(uploaded_keys.iter().any(|k| k.contains("data.json")));
    assert!(!uploaded_keys.iter().any(|k| k.contains("notes.txt")));
    assert_eq!(calls.len(), 3);
}

#[tokio::test]
async fn test_publish_nested_directories() {
    let client = MockS3Client::new();
    let source = tempfile::tempdir().unwrap();

    let subdir = source.path().join("sub/dir");
    std::fs::create_dir_all(&subdir).unwrap();
    std::fs::write(subdir.join("nested.png"), b"nested-image").unwrap();

    let storage = S3Storage::with_client(default_s3_config(), client.clone());
    storage.publish("key1", source.path()).await.unwrap();

    let calls = client.get_put_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].key, "key1/sub/dir/nested.png");
}

#[tokio::test]
async fn test_publish_with_acl() {
    let mut config = default_s3_config();
    config.acl = Some("public-read".to_string());

    let client = MockS3Client::new();
    let source = tempfile::tempdir().unwrap();
    std::fs::write(source.path().join("file.png"), b"data").unwrap();

    let storage = S3Storage::with_client(config, client.clone());
    storage.publish("key1", source.path()).await.unwrap();

    let calls = client.get_put_calls();
    assert_eq!(calls[0].params.acl.as_deref(), Some("public-read"));
}

#[tokio::test]
async fn test_publish_with_sse_kms() {
    let mut config = default_s3_config();
    config.sse_kms_key_id = Some("arn:aws:kms:us-east-1:123456:key/abc".to_string());

    let client = MockS3Client::new();
    let source = tempfile::tempdir().unwrap();
    std::fs::write(source.path().join("file.png"), b"data").unwrap();

    let storage = S3Storage::with_client(config, client.clone());
    storage.publish("key1", source.path()).await.unwrap();

    let calls = client.get_put_calls();
    match &calls[0].params.sse {
        Some(kaiki_storage::s3_client::SseConfig::Kms(key_id)) => {
            assert_eq!(key_id, "arn:aws:kms:us-east-1:123456:key/abc");
        }
        other => panic!("expected SseConfig::Kms, got {other:?}"),
    }
}

#[tokio::test]
async fn test_publish_with_sse_aes256() {
    let mut config = default_s3_config();
    config.sse = Some(true);

    let client = MockS3Client::new();
    let source = tempfile::tempdir().unwrap();
    std::fs::write(source.path().join("file.png"), b"data").unwrap();

    let storage = S3Storage::with_client(config, client.clone());
    storage.publish("key1", source.path()).await.unwrap();

    let calls = client.get_put_calls();
    assert!(
        matches!(calls[0].params.sse, Some(kaiki_storage::s3_client::SseConfig::Aes256)),
        "expected Aes256, got {:?}",
        calls[0].params.sse
    );
}

#[tokio::test]
async fn test_publish_report_url() {
    // Without path_prefix
    let client = MockS3Client::new();
    let source = tempfile::tempdir().unwrap();
    std::fs::write(source.path().join("index.html"), b"<html>").unwrap();

    let storage = S3Storage::with_client(default_s3_config(), client);
    let result = storage.publish("key1", source.path()).await.unwrap();
    assert_eq!(
        result.report_url.as_deref(),
        Some("https://test-bucket.s3.amazonaws.com/key1/index.html")
    );

    // With path_prefix
    let mut config = default_s3_config();
    config.path_prefix = Some("my-prefix".to_string());
    let client2 = MockS3Client::new();
    let source2 = tempfile::tempdir().unwrap();
    std::fs::write(source2.path().join("index.html"), b"<html>").unwrap();

    let storage2 = S3Storage::with_client(config, client2);
    let result2 = storage2.publish("key1", source2.path()).await.unwrap();
    assert_eq!(
        result2.report_url.as_deref(),
        Some("https://test-bucket.s3.amazonaws.com/my-prefix/key1/index.html")
    );
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn decompress_gzip(data: &[u8]) -> Vec<u8> {
    use flate2::read::GzDecoder;
    use std::io::Read;

    let mut decoder = GzDecoder::new(data);
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed).unwrap();
    decompressed
}

/// Verify all upload extensions are recognized.
#[test]
fn test_upload_extensions_coverage() {
    for ext in UPLOAD_EXTENSIONS {
        assert!(!ext.is_empty(), "empty extension in UPLOAD_EXTENSIONS");
    }
}
