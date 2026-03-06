use kaiki_git::{GitError, KeyGenerator};
use napi::bindgen_prelude::Promise;
use napi::threadsafe_function::ThreadsafeFunction;

/// A KeyGenerator backed by JS callback functions via ThreadsafeFunction.
///
/// `get_expected_key` and `get_actual_key` are JS async functions.
/// Since `KeyGenerator` is a sync trait, we use `futures::executor::block_on`
/// to await the JS Promise. This is safe because the caller runs on a
/// tokio/libuv worker thread, so the JS main thread is not blocked.
pub struct JsKeyGenerator {
    get_expected_key_fn: ThreadsafeFunction<()>,
    get_actual_key_fn: ThreadsafeFunction<()>,
}

impl JsKeyGenerator {
    pub fn new(
        get_expected_key_fn: ThreadsafeFunction<()>,
        get_actual_key_fn: ThreadsafeFunction<()>,
    ) -> Self {
        Self { get_expected_key_fn, get_actual_key_fn }
    }
}

impl KeyGenerator for JsKeyGenerator {
    fn get_expected_key(&self) -> Result<Option<String>, GitError> {
        let result: Option<String> = futures::executor::block_on(async {
            let promise: Promise<Option<String>> = self
                .get_expected_key_fn
                .call_async(Ok(()))
                .await
                .map_err(|e| GitError::Git(e.reason.clone()))?;
            promise.await.map_err(|e| GitError::Git(e.reason.clone()))
        })?;

        // Treat empty string as None (JS may return "" instead of null)
        if result.as_deref() == Some("") { Ok(None) } else { Ok(result) }
    }

    fn get_actual_key(&self) -> Result<String, GitError> {
        futures::executor::block_on(async {
            let promise: Promise<String> = self
                .get_actual_key_fn
                .call_async(Ok(()))
                .await
                .map_err(|e| GitError::Git(e.reason.clone()))?;
            promise.await.map_err(|e| GitError::Git(e.reason.clone()))
        })
    }
}
