use std::future::Future;

use crate::StorageError;

/// A single object entry returned from a GCS list operation.
#[derive(Debug, Clone)]
pub struct GcsObjectEntry {
    pub name: String,
    pub content_encoding: Option<String>,
}

/// Response from listing objects in a GCS bucket.
#[derive(Debug, Clone)]
pub struct GcsListOutput {
    pub objects: Vec<GcsObjectEntry>,
}

/// Trait abstracting GCS SDK operations for testability.
pub trait GcsClient: Clone + Send + Sync {
    fn list_objects(
        &self,
        bucket: &str,
        prefix: &str,
    ) -> impl Future<Output = Result<GcsListOutput, StorageError>> + Send;

    fn download_object(
        &self,
        bucket: &str,
        object: &str,
    ) -> impl Future<Output = Result<Vec<u8>, StorageError>> + Send;

    fn upload_object(
        &self,
        bucket: &str,
        name: &str,
        data: Vec<u8>,
        content_type: &str,
        content_encoding: &str,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;
}

/// Production GCS client wrapping the `google-cloud-storage` crate.
#[derive(Clone)]
pub struct HttpGcsClient {
    inner: google_cloud_storage::client::Client,
}

impl HttpGcsClient {
    pub fn new(inner: google_cloud_storage::client::Client) -> Self {
        Self { inner }
    }
}

impl GcsClient for HttpGcsClient {
    async fn list_objects(
        &self,
        bucket: &str,
        prefix: &str,
    ) -> Result<GcsListOutput, StorageError> {
        use google_cloud_storage::http::objects::list::ListObjectsRequest;

        let resp = self
            .inner
            .list_objects(&ListObjectsRequest {
                bucket: bucket.to_string(),
                prefix: Some(prefix.to_string()),
                ..Default::default()
            })
            .await
            .map_err(|e| StorageError::Gcs(Box::new(e)))?;

        let objects = resp
            .items
            .unwrap_or_default()
            .into_iter()
            .map(|obj| GcsObjectEntry {
                name: obj.name,
                content_encoding: obj.content_encoding,
            })
            .collect();

        Ok(GcsListOutput { objects })
    }

    async fn download_object(
        &self,
        bucket: &str,
        object: &str,
    ) -> Result<Vec<u8>, StorageError> {
        use google_cloud_storage::http::objects::download::Range;
        use google_cloud_storage::http::objects::get::GetObjectRequest;

        self.inner
            .download_object(
                &GetObjectRequest {
                    bucket: bucket.to_string(),
                    object: object.to_string(),
                    ..Default::default()
                },
                &Range::default(),
            )
            .await
            .map_err(|e| StorageError::Gcs(Box::new(e)))
    }

    async fn upload_object(
        &self,
        bucket: &str,
        name: &str,
        data: Vec<u8>,
        content_type: &str,
        content_encoding: &str,
    ) -> Result<(), StorageError> {
        use google_cloud_storage::http::objects::Object;
        use google_cloud_storage::http::objects::upload::{UploadObjectRequest, UploadType};

        let upload_type = UploadType::Multipart(Box::new(Object {
            name: name.to_string(),
            content_type: Some(content_type.to_string()),
            content_encoding: Some(content_encoding.to_string()),
            ..Default::default()
        }));

        self.inner
            .upload_object(
                &UploadObjectRequest { bucket: bucket.to_string(), ..Default::default() },
                data,
                &upload_type,
            )
            .await
            .map_err(|e| StorageError::Gcs(Box::new(e)))?;

        Ok(())
    }
}
