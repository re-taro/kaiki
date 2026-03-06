use std::io::Write as _;
use std::path::Path;
use std::sync::Arc;

use flate2::Compression;
use flate2::write::GzEncoder;
use google_cloud_storage::client::{Client, ClientConfig};
use google_cloud_storage::http::objects::download::Range;
use google_cloud_storage::http::objects::get::GetObjectRequest;
use google_cloud_storage::http::objects::list::ListObjectsRequest;
use google_cloud_storage::http::objects::upload::{UploadObjectRequest, UploadType};
use google_cloud_storage::http::objects::Object;
use kaiki_config::GcsPluginConfig;

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
pub struct GcsStorage {
    client: Client,
    config: GcsPluginConfig,
}

impl GcsStorage {
    pub async fn new(config: GcsPluginConfig) -> Result<Self, StorageError> {
        let gcs_config = ClientConfig::default()
            .with_auth()
            .await
            .map_err(|e| StorageError::Gcs(Box::new(e)))?;

        let client = Client::new(gcs_config);

        Ok(Self { client, config })
    }

    fn build_key(&self, storage_key: &str, relative_path: &str) -> String {
        build_gcs_key(self.config.path_prefix.as_deref(), storage_key, relative_path)
    }

    fn report_url(&self, storage_key: &str) -> String {
        gcs_report_url(&self.config.bucket_name, self.config.path_prefix.as_deref(), storage_key)
    }
}

impl crate::Storage for GcsStorage {
    async fn fetch(&self, key: &str, dest_dir: &Path) -> Result<(), StorageError> {
        let prefix = self.build_key(key, "");
        let semaphore = Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENCY));

        let objects = self
            .client
            .list_objects(&ListObjectsRequest {
                bucket: self.config.bucket_name.clone(),
                prefix: Some(prefix.clone()),
                ..Default::default()
            })
            .await
            .map_err(|e| StorageError::Gcs(Box::new(e)))?;

        let items = objects.items.unwrap_or_default();
        let mut handles = Vec::new();

        for obj in items {
            let obj_name = obj.name.clone();
            let relative = obj_name.strip_prefix(&prefix).unwrap_or(&obj_name).to_string();
            if relative.is_empty() {
                continue;
            }

            let content_encoding = obj.content_encoding.clone();
            let dest_path = dest_dir.join(&relative);
            let client = self.client.clone();
            let bucket = self.config.bucket_name.clone();
            let semaphore = Arc::clone(&semaphore);

            handles.push(tokio::spawn(async move {
                let _permit =
                    semaphore.acquire().await.map_err(|e| StorageError::Gcs(Box::new(e)))?;

                let bytes = client
                    .download_object(
                        &GetObjectRequest {
                            bucket,
                            object: obj_name,
                            ..Default::default()
                        },
                        &Range::default(),
                    )
                    .await
                    .map_err(|e| StorageError::Gcs(Box::new(e)))?;

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
            .filter_map(|e| e.ok())
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

            // Gzip compress
            let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
            encoder
                .write_all(&data)
                .map_err(|e| StorageError::Compression(Box::new(e)))?;
            let compressed =
                encoder.finish().map_err(|e| StorageError::Compression(Box::new(e)))?;

            let client = self.client.clone();
            let bucket = self.config.bucket_name.clone();
            let semaphore = Arc::clone(&semaphore);

            handles.push(tokio::spawn(async move {
                let _permit = semaphore
                    .acquire()
                    .await
                    .map_err(|e| StorageError::Gcs(Box::new(e)))?;

                let upload_type = UploadType::Multipart(Box::new(Object {
                    name: gcs_key.clone(),
                    content_type: Some(content_type),
                    content_encoding: Some("gzip".to_string()),
                    ..Default::default()
                }));

                client
                    .upload_object(
                        &UploadObjectRequest {
                            bucket,
                            ..Default::default()
                        },
                        compressed,
                        &upload_type,
                    )
                    .await
                    .map_err(|e| StorageError::Gcs(Box::new(e)))?;

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
        assert_eq!(
            url,
            "https://storage.googleapis.com/my-bucket/my-prefix/abc123/index.html"
        );
    }

    #[test]
    fn test_gcs_report_url_no_prefix() {
        let url = gcs_report_url("my-bucket", None, "abc123");
        assert_eq!(
            url,
            "https://storage.googleapis.com/my-bucket/abc123/index.html"
        );
    }

    #[test]
    fn test_gcs_report_url_empty_prefix() {
        let url = gcs_report_url("my-bucket", Some(""), "abc123");
        assert_eq!(
            url,
            "https://storage.googleapis.com/my-bucket/abc123/index.html"
        );
    }
}
