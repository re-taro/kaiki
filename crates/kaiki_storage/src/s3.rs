use std::path::Path;
use std::sync::Arc;

use flate2::Compression;
use flate2::write::GzEncoder;
use kaiki_config::S3PluginConfig;

use crate::s3_client::{HttpS3Client, PutObjectParams, S3Client, SseConfig};
use crate::{MAX_CONCURRENCY, PublishResult, StorageError, UPLOAD_EXTENSIONS, maybe_decompress};

/// Build an S3 object key from optional prefix, storage key, and relative path.
fn build_s3_key(path_prefix: Option<&str>, storage_key: &str, relative_path: &str) -> String {
    match path_prefix {
        Some(prefix) if !prefix.is_empty() => {
            format!("{prefix}/{storage_key}/{relative_path}")
        }
        _ => format!("{storage_key}/{relative_path}"),
    }
}

/// Build the public report URL for an S3-hosted report.
fn s3_report_url(bucket_name: &str, path_prefix: Option<&str>, storage_key: &str) -> String {
    let prefix = path_prefix.unwrap_or("");
    if prefix.is_empty() {
        format!("https://{bucket_name}.s3.amazonaws.com/{storage_key}/index.html")
    } else {
        format!("https://{bucket_name}.s3.amazonaws.com/{prefix}/{storage_key}/index.html")
    }
}

/// S3 storage backend.
pub struct S3Storage<C: S3Client> {
    client: C,
    config: S3PluginConfig,
}

impl S3Storage<HttpS3Client> {
    /// Creates a new S3 storage backend from the given plugin configuration.
    pub async fn new(config: S3PluginConfig) -> Result<Self, StorageError> {
        let mut aws_config_builder = aws_config::from_env();

        if let Some(ref region) = config.region {
            aws_config_builder =
                aws_config_builder.region(aws_sdk_s3::config::Region::new(region.clone()));
        }

        let aws_config = aws_config_builder.load().await;

        let mut s3_config_builder =
            aws_sdk_s3::config::Builder::from(&aws_config).force_path_style(true);

        if let Some(ref endpoint) = config.endpoint {
            s3_config_builder = s3_config_builder.endpoint_url(endpoint.clone());
        }

        let client = HttpS3Client::new(aws_sdk_s3::Client::from_conf(s3_config_builder.build()));

        Ok(Self { client, config })
    }
}

impl<C: S3Client> S3Storage<C> {
    /// Creates an S3 storage backend with a custom client (for testing).
    pub fn with_client(config: S3PluginConfig, client: C) -> Self {
        Self { client, config }
    }

    fn build_key(&self, storage_key: &str, relative_path: &str) -> String {
        build_s3_key(self.config.path_prefix.as_deref(), storage_key, relative_path)
    }

    fn report_url(&self, storage_key: &str) -> String {
        s3_report_url(&self.config.bucket_name, self.config.path_prefix.as_deref(), storage_key)
    }
}

impl<C: S3Client + 'static> crate::Storage for S3Storage<C> {
    async fn fetch(&self, key: &str, dest_dir: &Path) -> Result<(), StorageError> {
        let prefix = self.build_key(key, "");
        let semaphore = Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENCY));

        let mut continuation_token = None;

        loop {
            let resp = self
                .client
                .list_objects_v2(&self.config.bucket_name, &prefix, continuation_token.as_deref())
                .await?;

            let mut handles = Vec::new();

            for obj in resp.objects {
                let relative = obj.key.strip_prefix(&prefix).unwrap_or(&obj.key).to_string();
                if relative.is_empty() {
                    continue;
                }

                let dest_path = dest_dir.join(&relative);
                let client = self.client.clone();
                let bucket = self.config.bucket_name.clone();
                let obj_key = obj.key;
                let semaphore = Arc::clone(&semaphore);

                handles.push(tokio::spawn(async move {
                    let _permit =
                        semaphore.acquire().await.map_err(|e| StorageError::S3(Box::new(e)))?;

                    let output = client.get_object(&bucket, &obj_key).await?;

                    let data = maybe_decompress(&output.body, output.content_encoding.as_deref());

                    if let Some(parent) = dest_path.parent() {
                        tokio::fs::create_dir_all(parent).await.map_err(StorageError::Io)?;
                    }
                    tokio::fs::write(&dest_path, &data).await.map_err(StorageError::Io)?;

                    Ok::<(), StorageError>(())
                }));
            }

            for handle in handles {
                handle.await.map_err(|e| StorageError::S3(Box::new(e)))??;
            }

            if resp.is_truncated {
                continuation_token = resp.next_continuation_token;
            } else {
                break;
            }
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
            let s3_key = self.build_key(key, &relative);
            let content_type = mime_guess::from_path(path).first_or_octet_stream().to_string();

            let data = std::fs::read(path).map_err(StorageError::Io)?;

            let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
            std::io::Write::write_all(&mut encoder, &data)
                .map_err(|e| StorageError::Compression(Box::new(e)))?;
            let compressed =
                encoder.finish().map_err(|e| StorageError::Compression(Box::new(e)))?;

            let sse = self.config.sse.unwrap_or(false);
            let sse_kms_key_id = self.config.sse_kms_key_id.clone();
            let sse_config = if let Some(kms_key_id) = sse_kms_key_id {
                Some(SseConfig::Kms(kms_key_id))
            } else if sse {
                Some(SseConfig::Aes256)
            } else {
                None
            };

            let params = PutObjectParams {
                content_type,
                content_encoding: "gzip".to_string(),
                acl: self.config.acl.clone(),
                sse: sse_config,
            };

            let client = self.client.clone();
            let bucket = self.config.bucket_name.clone();
            let semaphore = Arc::clone(&semaphore);

            handles.push(tokio::spawn(async move {
                let _permit =
                    semaphore.acquire().await.map_err(|e| StorageError::S3(Box::new(e)))?;

                client.put_object(&bucket, &s3_key, compressed, params).await?;

                Ok::<(), StorageError>(())
            }));
        }

        for handle in handles {
            handle.await.map_err(|e| StorageError::S3(Box::new(e)))??;
        }

        Ok(PublishResult { report_url: Some(self.report_url(key)) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_s3_key_with_prefix() {
        let key = build_s3_key(Some("my-prefix"), "abc123", "screenshot.png");
        assert_eq!(key, "my-prefix/abc123/screenshot.png");
    }

    #[test]
    fn test_build_s3_key_no_prefix() {
        let key = build_s3_key(None, "abc123", "screenshot.png");
        assert_eq!(key, "abc123/screenshot.png");
    }

    #[test]
    fn test_build_s3_key_empty_prefix() {
        let key = build_s3_key(Some(""), "abc123", "screenshot.png");
        assert_eq!(key, "abc123/screenshot.png");
    }

    #[test]
    fn test_build_s3_key_nested_path() {
        let key = build_s3_key(Some("prefix"), "key1", "subdir/file.png");
        assert_eq!(key, "prefix/key1/subdir/file.png");
    }

    #[test]
    fn test_s3_report_url_with_prefix() {
        let url = s3_report_url("my-bucket", Some("my-prefix"), "abc123");
        assert_eq!(url, "https://my-bucket.s3.amazonaws.com/my-prefix/abc123/index.html");
    }

    #[test]
    fn test_s3_report_url_no_prefix() {
        let url = s3_report_url("my-bucket", None, "abc123");
        assert_eq!(url, "https://my-bucket.s3.amazonaws.com/abc123/index.html");
    }

    #[test]
    fn test_s3_report_url_empty_prefix() {
        let url = s3_report_url("my-bucket", Some(""), "abc123");
        assert_eq!(url, "https://my-bucket.s3.amazonaws.com/abc123/index.html");
    }
}
