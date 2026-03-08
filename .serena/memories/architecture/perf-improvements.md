# Performance Improvements Applied

## Changes (2026-03-05)

### High Priority (Done)

1. **`#[inline]` on hot path functions** - `color_delta`, `draw_pixel_*`, `is_antialiased`, `has_many_siblings`
2. **Bounds check optimization** - Slice `&img[k..k+4]` before individual indexing in `color_delta`, `draw_pixel_same`, `draw_pixel_diff`, `draw_pixel_aa`
3. **diff_image.data clone removed** - Move instead of clone in `RegProcessor::compare` (saves 8MB/image)
4. **MD5 → xxHash3** - `xxhash-rust` xxh3 for file identity fast path (~60x faster hashing)
5. **BTreeSet kept** - Decided against FxHashSet; BTreeSet gives deterministic report order
6. **dev profile optimization** - `[profile.dev.package.kaiki_diff] opt-level = 3`

### Medium Priority (Done)

7. **anyhow removed entirely** - CLI uses `CliError` (thiserror), routes sub-crate errors through `CoreError`
8. **StorageError improved** - `S3(String)` → `S3(Box<dyn Error + Send + Sync>)`, preserves error chain
9. **Rayon batch processing** - `common.chunks(concurrency * 4)` controls peak memory usage

### Error Architecture

- `CliError` → routes all sub-crate errors through `CoreError` via manual `From` impls
- `CoreError` wraps: `DiffError`, `ConfigError`, `ReportError`, `GitError`, `StorageError`, `NotifyError`, `io::Error`
- `CoreError::Other(String)` removed
- `anyhow` completely removed from workspace
