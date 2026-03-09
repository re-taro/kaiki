use std::path::Path;
use std::pin::Pin;

use futures::future::Future;
use kaiki_core::processor::StorageDyn;
use kaiki_storage::{PublishResult, StorageError};
use napi::bindgen_prelude::Promise;
use napi::threadsafe_function::ThreadsafeFunction;
use napi_derive::napi;

/// Arguments passed to the JS `fetch` callback.
#[napi(object)]
#[derive(Clone)]
pub struct JsFetchArgs {
    pub key: String,
    pub dest_dir: String,
}

/// Arguments passed to the JS `publish` callback.
#[napi(object)]
#[derive(Clone)]
pub struct JsPublishArgs {
    pub key: String,
    pub source_dir: String,
}

/// Result returned from the JS `publish` callback.
#[napi(object)]
#[derive(Clone)]
pub struct JsPublishResult {
    pub report_url: Option<String>,
}

/// A Storage backend backed by JS callback functions via ThreadsafeFunction.
///
/// Implements `StorageDyn` directly (not via the `Storage` blanket impl)
/// because `Storage` uses `impl Future` which is not object-safe.
pub struct JsStorage {
    fetch_fn: ThreadsafeFunction<JsFetchArgs, Promise<()>>,
    publish_fn: ThreadsafeFunction<JsPublishArgs, Promise<JsPublishResult>>,
}

impl JsStorage {
    pub fn new(
        fetch_fn: ThreadsafeFunction<JsFetchArgs, Promise<()>>,
        publish_fn: ThreadsafeFunction<JsPublishArgs, Promise<JsPublishResult>>,
    ) -> Self {
        Self { fetch_fn, publish_fn }
    }
}

impl StorageDyn for JsStorage {
    fn fetch_dyn<'a>(
        &'a self,
        key: &'a str,
        dest_dir: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), StorageError>> + Send + 'a>> {
        let args =
            JsFetchArgs { key: key.to_string(), dest_dir: dest_dir.to_string_lossy().into_owned() };
        Box::pin(async move {
            let promise: Promise<()> = self
                .fetch_fn
                .call_async(Ok(args))
                .await
                .map_err(|e| StorageError::Config(e.reason.clone()))?;
            promise.await.map_err(|e| StorageError::Config(e.reason.clone()))
        })
    }

    fn publish_dyn<'a>(
        &'a self,
        key: &'a str,
        source_dir: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<PublishResult, StorageError>> + Send + 'a>> {
        let args = JsPublishArgs {
            key: key.to_string(),
            source_dir: source_dir.to_string_lossy().into_owned(),
        };
        Box::pin(async move {
            let promise: Promise<JsPublishResult> = self
                .publish_fn
                .call_async(Ok(args))
                .await
                .map_err(|e| StorageError::Config(e.reason.clone()))?;
            let result = promise.await.map_err(|e| StorageError::Config(e.reason.clone()))?;
            Ok(PublishResult { report_url: result.report_url })
        })
    }
}
