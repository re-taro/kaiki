#![allow(clippy::print_stdout, clippy::print_stderr)]

mod adapters;

use compact_str::CompactString;
use kaiki_config::RegSuitConfiguration;
use kaiki_core::PipelineResult;
use kaiki_core::processor::RegProcessor;
use kaiki_report::ComparisonResult;
use napi::bindgen_prelude::*;
use napi::threadsafe_function::ThreadsafeFunction;
use napi::{Env, NapiValue};
use napi_derive::napi;

use crate::adapters::key_generator::JsKeyGenerator;
use crate::adapters::notifier::{JsNotifier, JsNotifyParams};
use crate::adapters::storage::{JsFetchArgs, JsPublishArgs, JsPublishResult, JsStorage};

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

/// Check if a JS value is null or undefined.
fn is_nullish(val: &Unknown<'_>) -> Result<bool> {
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
#[expect(deprecated)] // JsObject required: Object<'_> does not implement ToNapiValue
pub fn run(env: Env, options: Object<'_>) -> Result<napi::JsObject> {
    // 1. Extract and deserialize config
    let config_val: Unknown = options.get_named_property("config")?;
    let config_json: serde_json::Value = env.from_js_value(config_val)?;
    let reg_config: RegSuitConfiguration = serde_json::from_value(config_json)
        .map_err(|e| Error::new(Status::InvalidArg, format!("invalid config: {e}")))?;

    // 2. Extract key generator callbacks (ThreadsafeFunctions directly from JS)
    let kg_obj: Object = options.get_named_property("keyGenerator")?;
    let get_expected_key_fn: ThreadsafeFunction<(), Promise<Option<String>>> =
        kg_obj.get_named_property("getExpectedKey")?;
    let get_actual_key_fn: ThreadsafeFunction<(), Promise<String>> =
        kg_obj.get_named_property("getActualKey")?;
    let keygen = JsKeyGenerator::new(get_expected_key_fn, get_actual_key_fn);

    // 3. Extract publisher callbacks (optional)
    let storage = {
        let publisher_unknown: Unknown = options.get_named_property("publisher")?;
        if is_nullish(&publisher_unknown)? {
            None
        } else {
            let pub_obj = Object::from_unknown(publisher_unknown)?;
            let fetch_fn: ThreadsafeFunction<JsFetchArgs, Promise<()>> =
                pub_obj.get_named_property("fetch")?;
            let publish_fn: ThreadsafeFunction<JsPublishArgs, Promise<JsPublishResult>> =
                pub_obj.get_named_property("publish")?;
            Some(Box::new(JsStorage::new(fetch_fn, publish_fn))
                as Box<dyn kaiki_core::processor::StorageDyn>)
        }
    };

    // 4. Extract notifier callbacks (optional)
    let mut notifiers: Vec<Box<dyn kaiki_core::processor::NotifierDyn>> = Vec::new();
    let notifiers_unknown: Unknown = options.get_named_property("notifiers")?;
    if !is_nullish(&notifiers_unknown)? {
        let notifiers_arr = Object::from_unknown(notifiers_unknown)?;
        let length = notifiers_arr.get_array_length()?;
        for i in 0..length {
            let notifier_obj: Object = notifiers_arr.get_element(i)?;
            let notify_fn: ThreadsafeFunction<JsNotifyParams, Promise<()>> =
                notifier_obj.get_named_property("notify")?;
            notifiers.push(Box::new(JsNotifier::new(notify_fn)));
        }
    }

    // 5. Resolve working directory
    let working_dir = std::path::PathBuf::from(&reg_config.core.working_dir);

    // 6. Create deferred promise and spawn async work
    let (deferred, promise) = env.create_deferred()?;
    let promise_raw = promise.raw();

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

    // SAFETY: JsObject wraps the same napi_value as Object<'_>.
    // The value is returned to JS immediately and the runtime manages its lifetime.
    Ok(unsafe { napi::JsObject::from_raw_unchecked(env.raw(), promise_raw) })
}
