# Performance Optimizations Applied

## High Priority (Done)

- H1: `#[inline]` on `color_delta`, `draw_pixel_*`, `is_antialiased`, `has_many_siblings`
- H2: Bounds check optimization via slice pre-extraction (`&img[k..k+4]`)
- H3: Removed `.clone()` on `DiffResult.data` (move semantics via `if let Some(diff_image)`)
- H4: MD5 → xxHash3 for identical-bytes fast path
- H5: Kept BTreeSet (deterministic ordering needed for reports)
- H6: `[profile.dev.package.kaiki_diff] opt-level = 3`

## Medium Priority (Done)

- M7: Replaced anyhow with typed CliError (thiserror) in kaiki_cli
- M8: StorageError uses `Box<dyn Error + Send + Sync>` instead of `.to_string()`
- M9: Rayon batched processing (`chunks(concurrency * 4)`) for memory control

## Low Priority (Done)

- L10: Thread-local buffer pool — SKIPPED (buffers move into DiffResult via ownership)
- L11: CompactString for image filenames in ComparisonResult and BTreeSet
  - compact_str v0.8 with serde feature
  - ComparisonResult fields: Vec<CompactString>, CompactString for dir names
  - find_images returns BTreeSet<CompactString>
  - PathBuf::join needs `.as_str()` since CompactString doesn't impl AsRef<Path>
  - Vec::contains needs &CompactString (not &String) — use CompactString::from("...")
- L12: unsafe get_unchecked in compare_images hot loop
  - Fast-path equality check uses unchecked slice access
  - diff_mask uses get_unchecked_mut
  - debug_assert! on buffer lengths before loop
  - Each unsafe block needs `// SAFETY:` comment (clippy::undocumented_unsafe_blocks)
  - Combine multiple unchecked accesses in single unsafe block to satisfy lint
