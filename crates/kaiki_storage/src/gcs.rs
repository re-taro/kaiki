use std::io::Write as _;
use std::path::Path;
use std::sync::Arc;

use flate2::Compression;
use flate2::write::GzEncoder;
use kaiki_config::GcsPluginConfig;

use crate::gcs_client::{GcsClient, HttpGcsClient};
use crate::{MAX_CONCURRENCY, PublishResult, StorageError, UPLOAD_EXTENSIONS, maybe_decompress};

/// Build a GCS object key from optional prefix, storage key, and relative path.
fn build_gcs_key(path_prefix: Option<&str>, storage_key: &str, relative_path: &str) -> String {
    match path_prefix {
        Some(prefix) if !prefix.is_empty() => {
            format!("{prefix}/{storage_key}/{relative_path}")
        }
        _ => format!("{storage_key}/{relative_path}"),
    }
}

/// Build the public report URL for a GCS-hosted report.
fn gcs_report_url(bucket_name: &str, path_prefix: Option<&str>, storage_key: &str) -> String {
    let prefix = path_prefix.unwrap_or("");
    if prefix.is_empty() {
        format!("https://storage.googleapis.com/{bucket_name}/{storage_key}/index.html")
    } else {
        format!("https://storage.googleapis.com/{bucket_name}/{prefix}/{storage_key}/index.html")
    }
}

/// GCS storage backend using the `google-cloud-storage` crate.
pub struct GcsStorage<C: GcsClient> {
    client: C,
    config: GcsPluginConfig,
}

impl GcsStorage<HttpGcsClient> {
    /// Creates a new GCS storage backend from the given plugin configuration.
    pub async fn new(config: GcsPluginConfig) -> Result<Self, StorageError> {
        use google_cloud_storage::client::{Client, ClientConfig};

        let gcs_config = ClientConfig::default()
            .with_auth()
            .await
            .map_err(|e| StorageError::Gcs(Box::new(e)))?;

        let client = HttpGcsClient::new(Client::new(gcs_config));

        Ok(Self { client, config })
    }
}

impl<C: GcsClient> GcsStorage<C> {
    /// Creates a GCS storage backend with a custom client (for testing).
    pub fn with_client(config: GcsPluginConfig, client: C) -> Self {
        Self { client, config }
    }

    fn build_key(&self, storage_key: &str, relative_path: &str) -> String {
        build_gcs_key(self.config.path_prefix.as_deref(), storage_key, relative_path)
    }

    fn report_url(&self, storage_key: &str) -> String {
        gcs_report_url(&self.config.bucket_name, self.config.path_prefix.as_deref(), storage_key)
    }
}

impl<C: GcsClient + 'static> crate::Storage for GcsStorage<C> {
    async fn fetch(&self, key: &str, dest_dir: &Path) -> Result<(), StorageError> {
        let prefix = self.build_key(key, "");
        let semaphore = Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENCY));

        let list_output = self.client.list_objects(&self.config.bucket_name, &prefix).await?;

        let mut handles = Vec::new();

        for obj in list_output.objects {
            let relative = obj.name.strip_prefix(&prefix).unwrap_or(&obj.name).to_string();
            if relative.is_empty() {
                continue;
            }

            let content_encoding = obj.content_encoding;
            let dest_path = dest_dir.join(&relative);
            let client = self.client.clone();
            let bucket = self.config.bucket_name.clone();
            let obj_name = obj.name;
            let semaphore = Arc::clone(&semaphore);

            handles.push(tokio::spawn(async move {
                let _permit =
                    semaphore.acquire().await.map_err(|e| StorageError::Gcs(Box::new(e)))?;

                let bytes = client.download_object(&bucket, &obj_name).await?;

                let data = maybe_decompress(&bytes, content_encoding.as_deref());

                if let Some(parent) = dest_path.parent() {
                    tokio::fs::create_dir_all(parent).await.map_err(StorageError::Io)?;
                }
                tokio::fs::write(&dest_path, &data).await.map_err(StorageError::Io)?;

                Ok::<(), StorageError>(())
            }));
        }

        for handle in handles {
            handle.await.map_err(|e| StorageError::Gcs(Box::new(e)))??;
        }

        Ok(())
    }

    async fn publish(&self, key: &str, source_dir: &Path) -> Result<PublishResult, StorageError> {
        let semaphore = Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENCY));
        let mut handles = Vec::new();

        for entry in walkdir::WalkDir::new(source_dir)
            .into_iter()
            .filter_map(std::result::Result::ok)
            .filter(|e| !e.file_type().is_dir())
        {
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

            if !UPLOAD_EXTENSIONS.contains(&ext) {
                continue;
            }

            let relative =
                path.strip_prefix(source_dir).unwrap_or(path).to_string_lossy().to_string();
            let gcs_key = self.build_key(key, &relative);
            let content_type = mime_guess::from_path(path).first_or_octet_stream().to_string();

            let data = std::fs::read(path).map_err(StorageError::Io)?;

            let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(&data).map_err(|e| StorageError::Compression(Box::new(e)))?;
            let compressed =
                encoder.finish().map_err(|e| StorageError::Compression(Box::new(e)))?;

            let client = self.client.clone();
            let bucket = self.config.bucket_name.clone();
            let semaphore = Arc::clone(&semaphore);

            handles.push(tokio::spawn(async move {
                let _permit =
                    semaphore.acquire().await.map_err(|e| StorageError::Gcs(Box::new(e)))?;

                client.upload_object(&bucket, &gcs_key, compressed, &content_type, "gzip").await?;

                Ok::<(), StorageError>(())
            }));
        }

        for handle in handles {
            handle.await.map_err(|e| StorageError::Gcs(Box::new(e)))??;
        }

        Ok(PublishResult { report_url: Some(self.report_url(key)) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_gcs_key_with_prefix() {
        let key = build_gcs_key(Some("my-prefix"), "abc123", "screenshot.png");
        assert_eq!(key, "my-prefix/abc123/screenshot.png");
    }

    #[test]
    fn test_build_gcs_key_no_prefix() {
        let key = build_gcs_key(None, "abc123", "screenshot.png");
        assert_eq!(key, "abc123/screenshot.png");
    }

    #[test]
    fn test_build_gcs_key_empty_prefix() {
        let key = build_gcs_key(Some(""), "abc123", "screenshot.png");
        assert_eq!(key, "abc123/screenshot.png");
    }

    #[test]
    fn test_build_gcs_key_nested_path() {
        let key = build_gcs_key(Some("prefix"), "key1", "subdir/file.png");
        assert_eq!(key, "prefix/key1/subdir/file.png");
    }

    #[test]
    fn test_gcs_report_url_with_prefix() {
        let url = gcs_report_url("my-bucket", Some("my-prefix"), "abc123");
        assert_eq!(url, "https://storage.googleapis.com/my-bucket/my-prefix/abc123/index.html");
    }

    #[test]
    fn test_gcs_report_url_no_prefix() {
        let url = gcs_report_url("my-bucket", None, "abc123");
        assert_eq!(url, "https://storage.googleapis.com/my-bucket/abc123/index.html");
    }

    #[test]
    fn test_gcs_report_url_empty_prefix() {
        let url = gcs_report_url("my-bucket", Some(""), "abc123");
        assert_eq!(url, "https://storage.googleapis.com/my-bucket/abc123/index.html");
    }
}
