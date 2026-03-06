use std::pin::Pin;

use compact_str::CompactString;
use futures::future::Future;
use kaiki_core::processor::NotifierDyn;
use kaiki_notify::{NotifyError, NotifyParams};
use napi::bindgen_prelude::Promise;
use napi::threadsafe_function::ThreadsafeFunction;
use napi_derive::napi;

/// Notify parameters passed to the JS `notify` callback.
/// Mirrors `NotifyParams` but with JS-compatible types.
#[napi(object)]
#[derive(Clone)]
pub struct JsNotifyParams {
    pub failed_items: Vec<String>,
    pub new_items: Vec<String>,
    pub deleted_items: Vec<String>,
    pub passed_items: Vec<String>,
    pub report_url: Option<String>,
    pub current_sha: String,
    /// napi uses i64 for integer types.
    pub pr_number: Option<i64>,
}

impl JsNotifyParams {
    fn from_notify_params(params: &NotifyParams) -> Self {
        fn compact_to_strings(items: &[CompactString]) -> Vec<String> {
            items.iter().map(|s| s.to_string()).collect()
        }
        Self {
            failed_items: compact_to_strings(&params.comparison.failed_items),
            new_items: compact_to_strings(&params.comparison.new_items),
            deleted_items: compact_to_strings(&params.comparison.deleted_items),
            passed_items: compact_to_strings(&params.comparison.passed_items),
            report_url: params.report_url.clone(),
            current_sha: params.current_sha.clone(),
            pr_number: params.pr_number.map(|n| n as i64),
        }
    }
}

/// A Notifier backend backed by a JS callback function via ThreadsafeFunction.
///
/// Implements `NotifierDyn` directly (not via the `Notifier` blanket impl)
/// because `Notifier` uses `impl Future` which is not object-safe.
pub struct JsNotifier {
    notify_fn: ThreadsafeFunction<JsNotifyParams>,
}

impl JsNotifier {
    pub fn new(notify_fn: ThreadsafeFunction<JsNotifyParams>) -> Self {
        Self { notify_fn }
    }
}

impl NotifierDyn for JsNotifier {
    fn notify_dyn<'a>(
        &'a self,
        params: &'a NotifyParams,
    ) -> Pin<Box<dyn Future<Output = Result<(), NotifyError>> + Send + 'a>> {
        let js_params = JsNotifyParams::from_notify_params(params);
        Box::pin(async move {
            let promise: Promise<()> = self
                .notify_fn
                .call_async(Ok(js_params))
                .await
                .map_err(|e| NotifyError::Failed(e.reason.clone()))?;
            promise.await.map_err(|e| NotifyError::Failed(e.reason.clone()))
        })
    }
}
