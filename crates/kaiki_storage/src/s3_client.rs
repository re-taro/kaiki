use std::future::Future;

use crate::StorageError;

/// A single object entry returned from a list operation.
#[derive(Debug, Clone)]
pub struct ObjectEntry {
    pub key: String,
}

/// Response from listing objects in an S3 bucket.
#[derive(Debug, Clone)]
pub struct ListObjectsOutput {
    pub objects: Vec<ObjectEntry>,
    pub is_truncated: bool,
    pub next_continuation_token: Option<String>,
}

/// Response from getting an object from S3.
#[derive(Debug)]
pub struct GetObjectOutput {
    pub body: Vec<u8>,
    pub content_encoding: Option<String>,
}

/// Parameters for uploading an object to S3.
#[derive(Debug, Clone)]
pub struct PutObjectParams {
    pub content_type: String,
    pub content_encoding: String,
    pub acl: Option<String>,
    pub sse: Option<SseConfig>,
}

/// Server-side encryption configuration.
#[derive(Debug, Clone)]
pub enum SseConfig {
    Aes256,
    Kms(String),
}

/// Trait abstracting S3 SDK operations for testability.
pub trait S3Client: Clone + Send + Sync {
    fn list_objects_v2(
        &self,
        bucket: &str,
        prefix: &str,
        continuation_token: Option<&str>,
    ) -> impl Future<Output = Result<ListObjectsOutput, StorageError>> + Send;

    fn get_object(
        &self,
        bucket: &str,
        key: &str,
    ) -> impl Future<Output = Result<GetObjectOutput, StorageError>> + Send;

    fn put_object(
        &self,
        bucket: &str,
        key: &str,
        body: Vec<u8>,
        params: PutObjectParams,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;
}

/// Production S3 client wrapping the AWS SDK.
#[derive(Clone)]
pub struct HttpS3Client {
    inner: aws_sdk_s3::Client,
}

impl HttpS3Client {
    pub fn new(inner: aws_sdk_s3::Client) -> Self {
        Self { inner }
    }
}

impl S3Client for HttpS3Client {
    async fn list_objects_v2(
        &self,
        bucket: &str,
        prefix: &str,
        continuation_token: Option<&str>,
    ) -> Result<ListObjectsOutput, StorageError> {
        let mut req = self.inner.list_objects_v2().bucket(bucket).prefix(prefix);

        if let Some(token) = continuation_token {
            req = req.continuation_token(token);
        }

        let resp = req.send().await.map_err(|e| StorageError::S3(Box::new(e)))?;

        let objects = resp
            .contents()
            .iter()
            .filter_map(|obj| obj.key().map(|k| ObjectEntry { key: k.to_string() }))
            .collect();

        Ok(ListObjectsOutput {
            objects,
            is_truncated: resp.is_truncated() == Some(true),
            next_continuation_token: resp.next_continuation_token().map(std::string::ToString::to_string),
        })
    }

    async fn get_object(&self, bucket: &str, key: &str) -> Result<GetObjectOutput, StorageError> {
        let resp = self
            .inner
            .get_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| StorageError::S3(Box::new(e)))?;

        let content_encoding = resp.content_encoding().map(std::string::ToString::to_string);

        let body = resp.body.collect().await.map_err(|e| StorageError::S3(Box::new(e)))?;
        let bytes = body.into_bytes().to_vec();

        Ok(GetObjectOutput { body: bytes, content_encoding })
    }

    async fn put_object(
        &self,
        bucket: &str,
        key: &str,
        body: Vec<u8>,
        params: PutObjectParams,
    ) -> Result<(), StorageError> {
        let mut req = self
            .inner
            .put_object()
            .bucket(bucket)
            .key(key)
            .body(body.into())
            .content_type(params.content_type)
            .content_encoding(params.content_encoding);

        if let Some(ref acl_value) = params.acl {
            req = req.acl(acl_value.as_str().into());
        }

        match params.sse {
            Some(SseConfig::Kms(ref kms_key_id)) => {
                req = req
                    .server_side_encryption(aws_sdk_s3::types::ServerSideEncryption::AwsKms)
                    .ssekms_key_id(kms_key_id);
            }
            Some(SseConfig::Aes256) => {
                req = req.server_side_encryption(aws_sdk_s3::types::ServerSideEncryption::Aes256);
            }
            None => {}
        }

        req.send().await.map_err(|e| StorageError::S3(Box::new(e)))?;

        Ok(())
    }
}
