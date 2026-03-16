use std::collections::HashMap;
use std::io::Write;
use std::sync::{Arc, Mutex};

use flate2::Compression;
use flate2::write::GzEncoder;
use kaiki_config::GcsPluginConfig;
use kaiki_storage::Storage;
use kaiki_storage::gcs::GcsStorage;
use kaiki_storage::gcs_client::{GcsClient, GcsListOutput, GcsObjectEntry};
use kaiki_storage::StorageError;

// ---------------------------------------------------------------------------
// Mock GCS Client
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct MockGcsClient {
    objects: Arc<Mutex<HashMap<String, MockObject>>>,
    upload_calls: Arc<Mutex<Vec<UploadCall>>>,
    download_error: Arc<Mutex<Option<String>>>,
}

#[derive(Clone)]
struct MockObject {
    data: Vec<u8>,
    content_encoding: Option<String>,
}

#[derive(Debug, Clone)]
struct UploadCall {
    _bucket: String,
    name: String,
    data: Vec<u8>,
    _content_type: String,
    content_encoding: String,
}

impl MockGcsClient {
    fn new() -> Self {
        Self {
            objects: Arc::new(Mutex::new(HashMap::new())),
            upload_calls: Arc::new(Mutex::new(Vec::new())),
            download_error: Arc::new(Mutex::new(None)),
        }
    }

    fn add_object(&self, name: &str, data: Vec<u8>) {
        self.objects.lock().unwrap().insert(
            name.to_string(),
            MockObject { data, content_encoding: None },
        );
    }

    fn add_object_with_encoding(&self, name: &str, data: Vec<u8>, encoding: &str) {
        self.objects.lock().unwrap().insert(
            name.to_string(),
            MockObject { data, content_encoding: Some(encoding.to_string()) },
        );
    }

    fn get_upload_calls(&self) -> Vec<UploadCall> {
        self.upload_calls.lock().unwrap().clone()
    }
}

impl GcsClient for MockGcsClient {
    async fn list_objects(
        &self,
        _bucket: &str,
        prefix: &str,
    ) -> Result<GcsListOutput, StorageError> {
        let objects = self.objects.lock().unwrap();
        let entries: Vec<GcsObjectEntry> = objects
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(k, v)| GcsObjectEntry {
                name: k.clone(),
                content_encoding: v.content_encoding.clone(),
            })
            .collect();

        Ok(GcsListOutput { objects: entries })
    }

    async fn download_object(
        &self,
        _bucket: &str,
        object: &str,
    ) -> Result<Vec<u8>, StorageError> {
        if let Some(ref msg) = *self.download_error.lock().unwrap() {
            return Err(StorageError::Gcs(msg.clone().into()));
        }

        let objects = self.objects.lock().unwrap();
        objects
            .get(object)
            .map(|o| o.data.clone())
            .ok_or_else(|| StorageError::Gcs("object not found".into()))
    }

    async fn upload_object(
        &self,
        bucket: &str,
        name: &str,
        data: Vec<u8>,
        content_type: &str,
        content_encoding: &str,
    ) -> Result<(), StorageError> {
        self.upload_calls.lock().unwrap().push(UploadCall {
            _bucket: bucket.to_string(),
            name: name.to_string(),
            data,
            _content_type: content_type.to_string(),
            content_encoding: content_encoding.to_string(),
        });
        Ok(())
    }
}

fn default_gcs_config() -> GcsPluginConfig {
    GcsPluginConfig {
        bucket_name: "test-bucket".to_string(),
        path_prefix: None,
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
    let client = MockGcsClient::new();
    client.add_object("abc123/screenshot.png", b"image-data".to_vec());

    let storage = GcsStorage::with_client(default_gcs_config(), client);
    let dest = tempfile::tempdir().unwrap();

    storage.fetch("abc123", dest.path()).await.unwrap();

    let content = std::fs::read(dest.path().join("screenshot.png")).unwrap();
    assert_eq!(content, b"image-data");
}

#[tokio::test]
async fn test_fetch_multiple_files() {
    let client = MockGcsClient::new();
    client.add_object("key1/a.png", b"data-a".to_vec());
    client.add_object("key1/b.png", b"data-b".to_vec());
    client.add_object("key1/c.png", b"data-c".to_vec());

    let storage = GcsStorage::with_client(default_gcs_config(), client);
    let dest = tempfile::tempdir().unwrap();

    storage.fetch("key1", dest.path()).await.unwrap();

    assert_eq!(std::fs::read(dest.path().join("a.png")).unwrap(), b"data-a");
    assert_eq!(std::fs::read(dest.path().join("b.png")).unwrap(), b"data-b");
    assert_eq!(std::fs::read(dest.path().join("c.png")).unwrap(), b"data-c");
}

#[tokio::test]
async fn test_fetch_gzip_decompression() {
    let original = b"hello compressed world";
    let compressed = gzip_compress(original);

    let client = MockGcsClient::new();
    client.add_object_with_encoding("key1/file.png", compressed, "gzip");

    let storage = GcsStorage::with_client(default_gcs_config(), client);
    let dest = tempfile::tempdir().unwrap();

    storage.fetch("key1", dest.path()).await.unwrap();

    let content = std::fs::read(dest.path().join("file.png")).unwrap();
    assert_eq!(content, original);
}

#[tokio::test]
async fn test_fetch_nested_paths() {
    let client = MockGcsClient::new();
    client.add_object("key1/sub/dir/file.png", b"nested-data".to_vec());

    let storage = GcsStorage::with_client(default_gcs_config(), client);
    let dest = tempfile::tempdir().unwrap();

    storage.fetch("key1", dest.path()).await.unwrap();

    let content = std::fs::read(dest.path().join("sub/dir/file.png")).unwrap();
    assert_eq!(content, b"nested-data");
}

#[tokio::test]
async fn test_fetch_empty_list() {
    let client = MockGcsClient::new();

    let storage = GcsStorage::with_client(default_gcs_config(), client);
    let dest = tempfile::tempdir().unwrap();

    storage.fetch("key1", dest.path()).await.unwrap();

    let entries: Vec<_> = std::fs::read_dir(dest.path()).unwrap().collect();
    assert!(entries.is_empty());
}

// ---------------------------------------------------------------------------
// Publish tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_publish_single_file() {
    let client = MockGcsClient::new();
    let source = tempfile::tempdir().unwrap();

    std::fs::write(source.path().join("screenshot.png"), b"image-data").unwrap();

    let storage = GcsStorage::with_client(default_gcs_config(), client.clone());
    let result = storage.publish("abc123", source.path()).await.unwrap();
    assert!(result.report_url.is_some());

    let calls = client.get_upload_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "abc123/screenshot.png");
    assert_eq!(calls[0].content_encoding, "gzip");

    // Verify body is gzip-compressed
    let decompressed = decompress_gzip(&calls[0].data);
    assert_eq!(decompressed, b"image-data");
}

#[tokio::test]
async fn test_publish_filters_extensions() {
    let client = MockGcsClient::new();
    let source = tempfile::tempdir().unwrap();

    std::fs::write(source.path().join("image.png"), b"png-data").unwrap();
    std::fs::write(source.path().join("report.html"), b"html-data").unwrap();
    std::fs::write(source.path().join("notes.txt"), b"text-data").unwrap();
    std::fs::write(source.path().join("data.json"), b"json-data").unwrap();

    let storage = GcsStorage::with_client(default_gcs_config(), client.clone());
    storage.publish("key1", source.path()).await.unwrap();

    let calls = client.get_upload_calls();
    let uploaded_names: Vec<&str> = calls.iter().map(|c| c.name.as_str()).collect();

    assert!(uploaded_names.iter().any(|n| n.contains("image.png")));
    assert!(uploaded_names.iter().any(|n| n.contains("report.html")));
    assert!(uploaded_names.iter().any(|n| n.contains("data.json")));
    assert!(!uploaded_names.iter().any(|n| n.contains("notes.txt")));
    assert_eq!(calls.len(), 3);
}

#[tokio::test]
async fn test_publish_nested_directories() {
    let client = MockGcsClient::new();
    let source = tempfile::tempdir().unwrap();

    let subdir = source.path().join("sub/dir");
    std::fs::create_dir_all(&subdir).unwrap();
    std::fs::write(subdir.join("nested.png"), b"nested-image").unwrap();

    let storage = GcsStorage::with_client(default_gcs_config(), client.clone());
    storage.publish("key1", source.path()).await.unwrap();

    let calls = client.get_upload_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "key1/sub/dir/nested.png");
}

#[tokio::test]
async fn test_publish_report_url() {
    // Without path_prefix
    let client = MockGcsClient::new();
    let source = tempfile::tempdir().unwrap();
    std::fs::write(source.path().join("index.html"), b"<html>").unwrap();

    let storage = GcsStorage::with_client(default_gcs_config(), client);
    let result = storage.publish("key1", source.path()).await.unwrap();
    assert_eq!(
        result.report_url.as_deref(),
        Some("https://storage.googleapis.com/test-bucket/key1/index.html")
    );

    // With path_prefix
    let mut config = default_gcs_config();
    config.path_prefix = Some("my-prefix".to_string());
    let client2 = MockGcsClient::new();
    let source2 = tempfile::tempdir().unwrap();
    std::fs::write(source2.path().join("index.html"), b"<html>").unwrap();

    let storage2 = GcsStorage::with_client(config, client2);
    let result2 = storage2.publish("key1", source2.path()).await.unwrap();
    assert_eq!(
        result2.report_url.as_deref(),
        Some("https://storage.googleapis.com/test-bucket/my-prefix/key1/index.html")
    );
}

#[tokio::test]
async fn test_publish_error_propagation() {
    // Use a client that will fail on upload
    let client = FailingUploadGcsClient;
    let source = tempfile::tempdir().unwrap();
    std::fs::write(source.path().join("file.png"), b"data").unwrap();

    let storage = GcsStorage::with_client(default_gcs_config(), client);
    let result = storage.publish("key1", source.path()).await;

    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("GCS error"), "unexpected error: {err_msg}");
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

/// A GCS client that always fails on upload (for error propagation testing).
#[derive(Clone)]
struct FailingUploadGcsClient;

impl GcsClient for FailingUploadGcsClient {
    async fn list_objects(
        &self,
        _bucket: &str,
        _prefix: &str,
    ) -> Result<GcsListOutput, StorageError> {
        Ok(GcsListOutput { objects: vec![] })
    }

    async fn download_object(
        &self,
        _bucket: &str,
        _object: &str,
    ) -> Result<Vec<u8>, StorageError> {
        Ok(vec![])
    }

    async fn upload_object(
        &self,
        _bucket: &str,
        _name: &str,
        _data: Vec<u8>,
        _content_type: &str,
        _content_encoding: &str,
    ) -> Result<(), StorageError> {
        Err(StorageError::Gcs("simulated upload failure".into()))
    }
}
