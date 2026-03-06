#![allow(clippy::print_stdout, clippy::print_stderr)]

mod adapters;

use compact_str::CompactString;
use kaiki_config::RegSuitConfiguration;
use kaiki_core::PipelineResult;
use kaiki_core::processor::RegProcessor;
use kaiki_report::ComparisonResult;
use napi::bindgen_prelude::*;
use napi::{Env, JsFunction, JsObject, JsUnknown, NapiValue};
use napi_derive::napi;

use crate::adapters::key_generator::JsKeyGenerator;
use crate::adapters::notifier::{JsNotifier, JsNotifyParams};
use crate::adapters::storage::{JsFetchArgs, JsPublishArgs, JsStorage};

// ── Result types ────────────────────────────────────────────────────────

/// The comparison result returned to JS.
#[napi(object)]
pub struct JsComparisonResult {
    pub failed_items: Vec<String>,
    pub new_items: Vec<String>,
    pub deleted_items: Vec<String>,
    pub passed_items: Vec<String>,
    pub expected_items: Vec<String>,
    pub actual_items: Vec<String>,
    pub diff_items: Vec<String>,
    pub actual_dir: String,
    pub expected_dir: String,
    pub diff_dir: String,
}

impl From<ComparisonResult> for JsComparisonResult {
    fn from(c: ComparisonResult) -> Self {
        fn to_strings(items: Vec<CompactString>) -> Vec<String> {
            items.into_iter().map(|s| s.to_string()).collect()
        }
        Self {
            failed_items: to_strings(c.failed_items),
            new_items: to_strings(c.new_items),
            deleted_items: to_strings(c.deleted_items),
            passed_items: to_strings(c.passed_items),
            expected_items: to_strings(c.expected_items),
            actual_items: to_strings(c.actual_items),
            diff_items: to_strings(c.diff_items),
            actual_dir: c.actual_dir.to_string(),
            expected_dir: c.expected_dir.to_string(),
            diff_dir: c.diff_dir.to_string(),
        }
    }
}

/// The full pipeline result returned to JS.
#[napi(object)]
pub struct JsPipelineResult {
    pub comparison: JsComparisonResult,
    pub report_url: Option<String>,
    pub has_failures: bool,
}

impl From<PipelineResult> for JsPipelineResult {
    fn from(r: PipelineResult) -> Self {
        Self {
            comparison: r.comparison.into(),
            report_url: r.report_url,
            has_failures: r.has_failures,
        }
    }
}

// ── Helper functions ────────────────────────────────────────────────────

/// Extract a named JS function property from an object and convert to ThreadsafeFunction.
fn extract_tsfn<T: 'static + Send + ToNapiValue>(
    env: &Env,
    obj: &JsObject,
    name: &str,
) -> Result<napi::threadsafe_function::ThreadsafeFunction<T>> {
    let func: JsFunction = obj.get_named_property(name)?;
    env.create_threadsafe_function(
        &func,
        0,
        |ctx: napi::threadsafe_function::ThreadSafeCallContext<T>| {
            // SAFETY: `to_napi_value` requires raw env and owned value; both come
            // from the ThreadSafeCallContext which guarantees validity.
            let val = unsafe { <T as ToNapiValue>::to_napi_value(ctx.env.raw(), ctx.value)? };
            // SAFETY: `val` was just created from the same env above.
            Ok(vec![unsafe { JsUnknown::from_raw_unchecked(ctx.env.raw(), val) }])
        },
    )
}

/// Extract a named JS function for zero-arg callbacks (like getExpectedKey/getActualKey).
fn extract_tsfn_no_args(
    env: &Env,
    obj: &JsObject,
    name: &str,
) -> Result<napi::threadsafe_function::ThreadsafeFunction<()>> {
    let func: JsFunction = obj.get_named_property(name)?;
    env.create_threadsafe_function(
        &func,
        0,
        |_ctx: napi::threadsafe_function::ThreadSafeCallContext<()>| {
            Ok::<Vec<JsUnknown>, napi::Error>(vec![])
        },
    )
}

/// Check if a JsUnknown value is null or undefined.
fn is_nullish(val: &JsUnknown) -> Result<bool> {
    let value_type = val.get_type()?;
    Ok(value_type == napi::ValueType::Null || value_type == napi::ValueType::Undefined)
}

// ── Main entry point ────────────────────────────────────────────────────

/// Run the kaiki visual regression testing pipeline with JS plugin callbacks.
///
/// This is the main entry point for the napi bindings. The `options` object
/// contains the config and JS callback functions for key generation, storage,
/// and notification.
///
/// The function manually extracts JS functions from the options object and
/// converts them to ThreadsafeFunctions that can be called from Rust threads.
#[napi(
    ts_args_type = "options: {
    config: object;
    keyGenerator: {
        getExpectedKey: () => Promise<string | null | undefined>;
        getActualKey: () => Promise<string>;
    };
    publisher?: {
        fetch: (args: { key: string; destDir: string }) => Promise<void>;
        publish: (args: { key: string; sourceDir: string }) => Promise<{ reportUrl?: string | null }>;
    };
    notifiers?: Array<{
        notify: (params: {
            failedItems: string[];
            newItems: string[];
            deletedItems: string[];
            passedItems: string[];
            reportUrl?: string | null;
            currentSha: string;
            prNumber?: number | null;
        }) => Promise<void>;
    }>;
}",
    ts_return_type = "Promise<{
    comparison: {
        failedItems: string[];
        newItems: string[];
        deletedItems: string[];
        passedItems: string[];
        expectedItems: string[];
        actualItems: string[];
        diffItems: string[];
        actualDir: string;
        expectedDir: string;
        diffDir: string;
    };
    reportUrl?: string | null;
    hasFailures: boolean;
}>"
)]
pub fn run(env: Env, options: JsObject) -> Result<JsObject> {
    // 1. Extract and deserialize config
    let config_val: JsUnknown = options.get_named_property("config")?;
    let config_json: serde_json::Value = env.from_js_value(&config_val)?;
    let reg_config: RegSuitConfiguration = serde_json::from_value(config_json)
        .map_err(|e| Error::new(Status::InvalidArg, format!("invalid config: {e}")))?;

    // 2. Extract key generator callbacks
    let kg_obj: JsObject = options.get_named_property("keyGenerator")?;
    let get_expected_key_fn = extract_tsfn_no_args(&env, &kg_obj, "getExpectedKey")?;
    let get_actual_key_fn = extract_tsfn_no_args(&env, &kg_obj, "getActualKey")?;
    let keygen = JsKeyGenerator::new(get_expected_key_fn, get_actual_key_fn);

    // 3. Extract publisher callbacks (optional)
    let storage = {
        let publisher_unknown: JsUnknown = options.get_named_property("publisher")?;
        if is_nullish(&publisher_unknown)? {
            None
        } else {
            let pub_obj: JsObject = JsObject::from_unknown(publisher_unknown)?;
            let fetch_fn = extract_tsfn::<JsFetchArgs>(&env, &pub_obj, "fetch")?;
            let publish_fn = extract_tsfn::<JsPublishArgs>(&env, &pub_obj, "publish")?;
            Some(Box::new(JsStorage::new(fetch_fn, publish_fn))
                as Box<dyn kaiki_core::processor::StorageDyn>)
        }
    };

    // 4. Extract notifier callbacks (optional)
    let mut notifiers: Vec<Box<dyn kaiki_core::processor::NotifierDyn>> = Vec::new();
    let notifiers_unknown: JsUnknown = options.get_named_property("notifiers")?;
    if !is_nullish(&notifiers_unknown)? {
        let notifiers_arr = JsObject::from_unknown(notifiers_unknown)?;
        let length = notifiers_arr.get_array_length()?;
        for i in 0..length {
            let notifier_obj: JsObject = notifiers_arr.get_element(i)?;
            let notify_fn = extract_tsfn::<JsNotifyParams>(&env, &notifier_obj, "notify")?;
            notifiers.push(Box::new(JsNotifier::new(notify_fn)));
        }
    }

    // 5. Resolve working directory
    let working_dir = std::path::PathBuf::from(&reg_config.core.working_dir);

    // 6. Create deferred promise and spawn async work
    let (deferred, promise) = env.create_deferred()?;

    napi::bindgen_prelude::spawn(async move {
        let processor =
            RegProcessor::new(reg_config.core, working_dir, Box::new(keygen), storage, notifiers);

        match processor.run().await {
            Ok(result) => {
                let js_result: JsPipelineResult = result.into();
                deferred.resolve(|_env| Ok(js_result));
            }
            Err(e) => {
                deferred.reject(Error::new(Status::GenericFailure, format!("{e}")));
            }
        }
    });

    Ok(promise)
}
