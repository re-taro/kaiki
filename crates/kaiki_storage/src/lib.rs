pub mod gcs;
pub mod s3;

use std::path::Path;

use thiserror::Error;

/// Errors that can occur in storage operations.
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("S3 error: {0}")]
    S3(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error("GCS error: {0}")]
    Gcs(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error("compression error: {0}")]
    Compression(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error("configuration error: {0}")]
    Config(String),
}

/// Result of a publish operation.
#[derive(Debug, Clone)]
pub struct PublishResult {
    pub report_url: Option<String>,
}

/// Trait for storage backends (S3, GCS).
pub trait Storage: Send + Sync {
    /// Fetch expected images from storage to a local directory.
    fn fetch(
        &self,
        key: &str,
        dest_dir: &Path,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Publish comparison results to storage.
    fn publish(
        &self,
        key: &str,
        source_dir: &Path,
    ) -> impl Future<Output = Result<PublishResult, StorageError>> + Send;
}

use std::future::Future;

/// Decompress data if the content encoding indicates gzip.
/// Falls back to raw bytes on decompression failure.
pub(crate) fn maybe_decompress(data: &[u8], content_encoding: Option<&str>) -> Vec<u8> {
    use std::io::Read as _;

    use flate2::read::GzDecoder;

    let is_gzip =
        content_encoding.map(|enc| enc.to_ascii_lowercase().contains("gzip")).unwrap_or(false);

    if !is_gzip {
        return data.to_vec();
    }

    let mut decoder = GzDecoder::new(data);
    let mut decompressed = Vec::new();
    if decoder.read_to_end(&mut decompressed).is_ok() { decompressed } else { data.to_vec() }
}

/// File extensions to include when uploading.
pub const UPLOAD_EXTENSIONS: &[&str] =
    &["html", "js", "wasm", "png", "json", "jpeg", "jpg", "tiff", "bmp", "gif"];

/// Maximum concurrent uploads/downloads.
pub const MAX_CONCURRENCY: usize = 50;

#[cfg(test)]
mod tests {
    use std::io::Write;

    use flate2::Compression;
    use flate2::write::GzEncoder;

    use super::*;

    fn gzip_compress(data: &[u8]) -> Vec<u8> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap()
    }

    #[test]
    fn test_maybe_decompress_gzip_header() {
        let original = b"hello world";
        let compressed = gzip_compress(original);
        let result = maybe_decompress(&compressed, Some("gzip"));
        assert_eq!(result, original);
    }

    #[test]
    fn test_maybe_decompress_no_header() {
        let data = b"raw bytes";
        let result = maybe_decompress(data, None);
        assert_eq!(result, data);
    }

    #[test]
    fn test_maybe_decompress_no_encoding_string() {
        let data = b"raw bytes";
        let result = maybe_decompress(data, Some("identity"));
        assert_eq!(result, data);
    }

    #[test]
    fn test_maybe_decompress_corrupt_data_with_gzip_header() {
        let corrupt = b"not valid gzip data";
        let result = maybe_decompress(corrupt, Some("gzip"));
        // Falls back to raw bytes
        assert_eq!(result, corrupt);
    }
}
