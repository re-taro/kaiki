use std::path::{Path, PathBuf};

use compact_str::CompactString;
use kaiki_config::CoreConfig;
use kaiki_diff::CompareOptions;
use kaiki_git::KeyGenerator;
use kaiki_notify::NotifyParams;
use kaiki_report::ComparisonResult;
use rayon::prelude::*;

use crate::image_finder::find_images;
use crate::{CoreError, PipelineResult};

/// Main processor that orchestrates the visual regression testing pipeline.
pub struct RegProcessor {
    config: CoreConfig,
    working_dir: PathBuf,
    keygen: Box<dyn KeyGenerator>,
    storage: Option<Box<dyn StorageDyn>>,
    notifiers: Vec<Box<dyn NotifierDyn>>,
}

/// Object-safe wrapper for Storage trait.
pub trait StorageDyn: Send + Sync {
    fn fetch_dyn<'a>(
        &'a self,
        key: &'a str,
        dest_dir: &'a Path,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<(), kaiki_storage::StorageError>> + Send + 'a>>;

    fn publish_dyn<'a>(
        &'a self,
        key: &'a str,
        source_dir: &'a Path,
    ) -> std::pin::Pin<
        Box<
            dyn Future<Output = Result<kaiki_storage::PublishResult, kaiki_storage::StorageError>>
                + Send
                + 'a,
        >,
    >;
}

/// Object-safe wrapper for Notifier trait.
pub trait NotifierDyn: Send + Sync {
    fn notify_dyn<'a>(
        &'a self,
        params: &'a NotifyParams,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<(), kaiki_notify::NotifyError>> + Send + 'a>>;
}

impl<T: kaiki_storage::Storage> StorageDyn for T {
    fn fetch_dyn<'a>(
        &'a self,
        key: &'a str,
        dest_dir: &'a Path,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<(), kaiki_storage::StorageError>> + Send + 'a>>
    {
        Box::pin(self.fetch(key, dest_dir))
    }

    fn publish_dyn<'a>(
        &'a self,
        key: &'a str,
        source_dir: &'a Path,
    ) -> std::pin::Pin<
        Box<
            dyn Future<Output = Result<kaiki_storage::PublishResult, kaiki_storage::StorageError>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(self.publish(key, source_dir))
    }
}

impl<T: kaiki_notify::Notifier> NotifierDyn for T {
    fn notify_dyn<'a>(
        &'a self,
        params: &'a NotifyParams,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<(), kaiki_notify::NotifyError>> + Send + 'a>>
    {
        Box::pin(self.notify(params))
    }
}

use std::future::Future;

impl RegProcessor {
    pub fn new(
        config: CoreConfig,
        working_dir: PathBuf,
        keygen: Box<dyn KeyGenerator>,
        storage: Option<Box<dyn StorageDyn>>,
        notifiers: Vec<Box<dyn NotifierDyn>>,
    ) -> Self {
        Self { config, working_dir, keygen, storage, notifiers }
    }

    /// Run the full pipeline: get expected key → sync → compare → publish → notify.
    pub async fn run(&self) -> Result<PipelineResult, CoreError> {
        // 1. Get expected key
        let expected_key = self.get_expected_key()?;
        tracing::info!(expected_key = ?expected_key, "resolved expected key");

        // 2. Sync expected images
        if let Some(key) = &expected_key {
            self.sync_expected(key).await?;
        }

        // 3. Compare
        let comparison = self.compare()?;
        let has_failures = comparison.has_failures();

        // 4. Publish
        let report_url = if let Ok(key) = self.keygen.get_actual_key() {
            self.publish(&key).await?
        } else {
            None
        };

        // 5. Notify
        let actual_key = self.keygen.get_actual_key().unwrap_or_default();
        let notify_params = NotifyParams {
            comparison: comparison.clone(),
            report_url: report_url.clone(),
            current_sha: actual_key,
            pr_number: crate::ci::detect_pr_number(),
        };
        self.notify(&notify_params).await?;

        Ok(PipelineResult { comparison, report_url, has_failures })
    }

    /// Get the expected (baseline) key.
    pub fn get_expected_key(&self) -> Result<Option<String>, CoreError> {
        Ok(self.keygen.get_expected_key()?)
    }

    /// Download expected images from storage.
    pub async fn sync_expected(&self, key: &str) -> Result<(), CoreError> {
        let expected_dir = self.working_dir.join("expected");
        std::fs::create_dir_all(&expected_dir)?;

        if let Some(storage) = &self.storage {
            storage.fetch_dyn(key, &expected_dir).await?;
        }

        Ok(())
    }

    /// Compare actual images against expected images using rayon for parallelism.
    pub fn compare(&self) -> Result<ComparisonResult, CoreError> {
        let actual_dir = PathBuf::from(&self.config.actual_dir);
        let expected_dir = self.working_dir.join("expected");
        let diff_dir = self.working_dir.join("diff");

        // Copy actual images to working dir
        let actual_work_dir = self.working_dir.join("actual");
        if actual_dir.exists() {
            copy_dir(&actual_dir, &actual_work_dir)?;
        }

        std::fs::create_dir_all(&diff_dir)?;

        let actual_images = find_images(&actual_work_dir);
        let expected_images = find_images(&expected_dir);

        // Categorize images
        let common: Vec<_> = actual_images.intersection(&expected_images).cloned().collect();
        let new_items: Vec<_> = actual_images.difference(&expected_images).cloned().collect();
        let deleted_items: Vec<_> = expected_images.difference(&actual_images).cloned().collect();

        let options = CompareOptions {
            matching_threshold: kaiki_config::effective_matching_threshold(&self.config),
            enable_antialias: self.config.enable_antialias.unwrap_or(false),
            diff_color: self.config.diff_color.unwrap_or([255, 119, 119]),
            diff_color_alt: self.config.diff_color_alt,
            aa_color: self.config.aa_color.unwrap_or([255, 255, 0]),
            alpha: self.config.alpha.unwrap_or(0.1),
        };

        let threshold_rate = kaiki_config::effective_threshold_rate(&self.config);
        let threshold_pixel = self.config.threshold_pixel;

        // Build a custom rayon thread pool with configured concurrency
        let concurrency = kaiki_config::effective_concurrency(&self.config) as usize;
        let pool =
            rayon::ThreadPoolBuilder::new().num_threads(concurrency).build().unwrap_or_else(|_| {
                rayon::ThreadPoolBuilder::new().build().expect("failed to build default rayon pool")
            });

        // Parallel comparison in batches to control peak memory usage.
        // Each batch processes `concurrency * 4` images, then releases their buffers
        // before starting the next batch.
        let batch_size = concurrency * 4;
        let mut results: Vec<(CompactString, bool)> = Vec::with_capacity(common.len());

        for chunk in common.chunks(batch_size) {
            let batch: Vec<_> = pool.install(|| {
                chunk
                    .par_iter()
                    .map(|name| {
                        let actual_path = actual_work_dir.join(name.as_str());
                        let expected_path = expected_dir.join(name.as_str());

                        let actual_bytes = std::fs::read(&actual_path).ok();
                        let expected_bytes = std::fs::read(&expected_path).ok();

                        match (actual_bytes, expected_bytes) {
                            (Some(actual), Some(expected)) => {
                                match kaiki_diff::compare_image_files(&actual, &expected, &options)
                                {
                                    Ok(result) => {
                                        let passed = kaiki_report::is_passed(
                                            result.diff_count,
                                            result.total_pixels,
                                            threshold_pixel,
                                            threshold_rate,
                                        );

                                        // Write diff image
                                        if let Some(diff_image) = result.diff_image {
                                            let diff_path = diff_dir.join(name.as_str());
                                            if let Some(parent) = diff_path.parent() {
                                                let _ = std::fs::create_dir_all(parent);
                                            }
                                            let img = image::RgbaImage::from_raw(
                                                diff_image.width,
                                                diff_image.height,
                                                diff_image.data,
                                            );
                                            if let Some(img) = img {
                                                let _ = img.save(&diff_path);
                                            }
                                        }

                                        (name.clone(), passed)
                                    }
                                    Err(_) => (name.clone(), false),
                                }
                            }
                            _ => (name.clone(), false),
                        }
                    })
                    .collect()
            });
            results.extend(batch);
        }

        let mut failed_items = Vec::new();
        let mut passed_items = Vec::new();
        let mut diff_items = Vec::new();

        for (name, passed) in results {
            diff_items.push(name.clone());
            if passed {
                passed_items.push(name);
            } else {
                failed_items.push(name);
            }
        }

        let comparison = ComparisonResult {
            failed_items,
            new_items,
            deleted_items,
            passed_items,
            expected_items: expected_images.into_iter().collect(),
            actual_items: actual_images.into_iter().collect(),
            diff_items,
            actual_dir: "actual".into(),
            expected_dir: "expected".into(),
            diff_dir: "diff".into(),
        };

        // Write reports
        let json_path = self.working_dir.join("out.json");
        kaiki_report::write_json_report(&comparison, &json_path)?;

        let ximgdiff_enabled =
            self.config.ximgdiff.as_ref().and_then(|x| x.enabled).unwrap_or(false);

        let html_path = self.working_dir.join("index.html");
        kaiki_report::write_html_report(&comparison, &html_path, ximgdiff_enabled)?;

        if ximgdiff_enabled {
            kaiki_report::write_ximgdiff_assets(&self.working_dir)?;
        }

        Ok(comparison)
    }

    /// Get the actual (current) key.
    pub fn get_actual_key(&self) -> Result<String, CoreError> {
        Ok(self.keygen.get_actual_key()?)
    }

    /// Publish comparison results to storage.
    pub async fn publish(&self, key: &str) -> Result<Option<String>, CoreError> {
        if let Some(storage) = &self.storage {
            let result = storage.publish_dyn(key, &self.working_dir).await?;
            Ok(result.report_url)
        } else {
            Ok(None)
        }
    }

    /// Send notifications.
    pub async fn notify(&self, params: &NotifyParams) -> Result<(), CoreError> {
        for notifier in &self.notifiers {
            if let Err(e) = notifier.notify_dyn(params).await {
                // Notification failures should not break the pipeline
                tracing::warn!(error = %e, "notification failed");
            }
        }
        Ok(())
    }
}

/// Recursively copy a directory.
fn copy_dir(src: &Path, dst: &Path) -> Result<(), std::io::Error> {
    std::fs::create_dir_all(dst)?;
    for entry in walkdir::WalkDir::new(src).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        let relative = path.strip_prefix(src).unwrap_or(path);
        let dest = dst.join(relative);

        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&dest)?;
        } else {
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(path, &dest)?;
        }
    }
    Ok(())
}
