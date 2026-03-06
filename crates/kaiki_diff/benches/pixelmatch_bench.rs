use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use kaiki_diff::{CompareOptions, ImageData};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn fixture(name: &str) -> Vec<u8> {
    let path = format!("{}/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|_| panic!("fixture not found: {path}"))
}

fn decode(raw: &[u8]) -> ImageData {
    let img =
        image::load_from_memory(raw).unwrap_or_else(|e| panic!("failed to decode image: {e}"));
    let rgba = img.to_rgba8();
    ImageData {
        width: rgba.width(),
        height: rgba.height(),
        data: rgba.into_raw(),
    }
}

fn make_solid(w: u32, h: u32, r: u8, g: u8, b: u8) -> ImageData {
    let pixels = (w as usize) * (h as usize);
    let mut data = Vec::with_capacity(pixels * 4);
    for _ in 0..pixels {
        data.extend_from_slice(&[r, g, b, 255]);
    }
    ImageData {
        width: w,
        height: h,
        data,
    }
}

/// Deterministic pseudo-noise: mutate ~25 % of pixels using a simple hash.
fn make_noisy(base: &ImageData) -> ImageData {
    let mut data = base.data.clone();
    let pixels = (base.width as usize) * (base.height as usize);
    for i in 0..pixels {
        // Simple hash: Knuth multiplicative hash
        let hash = (i as u32).wrapping_mul(2_654_435_761);
        if hash.is_multiple_of(4) {
            // ~25 % of pixels
            let off = i * 4;
            data[off] = data[off].wrapping_add(40);
            data[off + 1] = data[off + 1].wrapping_add(40);
            data[off + 2] = data[off + 2].wrapping_add(40);
        }
    }
    ImageData {
        width: base.width,
        height: base.height,
        data,
    }
}

// ---------------------------------------------------------------------------
// 1. e2e/fixture — end-to-end (decode + compare)
// ---------------------------------------------------------------------------

fn bench_e2e_fixture(c: &mut Criterion) {
    let mut group = c.benchmark_group("e2e/fixture");

    let img_1a = fixture("1a.png");
    let img_1b = fixture("1b.png");
    let img_4a = fixture("4a.png");
    let img_4b = fixture("4b.png");
    let opts = CompareOptions::default();

    group.bench_function("1a-1b", |b| {
        b.iter(|| kaiki_diff::compare_image_files(&img_1a, &img_1b, &opts).unwrap());
    });
    group.bench_function("4a-4b", |b| {
        b.iter(|| kaiki_diff::compare_image_files(&img_4a, &img_4b, &opts).unwrap());
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// 2. e2e/hash-fastpath — identical file fast path
// ---------------------------------------------------------------------------

fn bench_e2e_hash_fastpath(c: &mut Criterion) {
    let img = fixture("6a.png");
    let opts = CompareOptions::default();

    c.bench_function("e2e/hash-fastpath (6a==6a)", |b| {
        b.iter(|| kaiki_diff::compare_image_files(&img, &img, &opts).unwrap());
    });
}

// ---------------------------------------------------------------------------
// 3. decode — image decoding only
// ---------------------------------------------------------------------------

fn bench_decode(c: &mut Criterion) {
    let mut group = c.benchmark_group("decode");

    let raw_1a = fixture("1a.png");
    let raw_4a = fixture("4a.png");

    group.bench_function("1a.png", |b| {
        b.iter(|| {
            let img = image::load_from_memory(&raw_1a).unwrap();
            let _ = img.to_rgba8();
        });
    });
    group.bench_function("4a.png", |b| {
        b.iter(|| {
            let img = image::load_from_memory(&raw_4a).unwrap();
            let _ = img.to_rgba8();
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// 4. compare_images/fixture — pure pixel comparison (pre-decoded)
// ---------------------------------------------------------------------------

fn bench_compare_fixture(c: &mut Criterion) {
    let mut group = c.benchmark_group("compare_images/fixture");

    let actual_1 = decode(&fixture("1a.png"));
    let expected_1 = decode(&fixture("1b.png"));
    let actual_4 = decode(&fixture("4a.png"));
    let expected_4 = decode(&fixture("4b.png"));
    let opts = CompareOptions::default();

    let px_1 = (actual_1.width as u64) * (actual_1.height as u64);
    let px_4 = (actual_4.width as u64) * (actual_4.height as u64);

    group.throughput(Throughput::Elements(px_1));
    group.bench_function("1a-1b", |b| {
        b.iter(|| kaiki_diff::compare_images(&actual_1, &expected_1, &opts));
    });

    group.throughput(Throughput::Elements(px_4));
    group.bench_function("4a-4b", |b| {
        b.iter(|| kaiki_diff::compare_images(&actual_4, &expected_4, &opts));
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// 5. compare_images/identical — best case (all pixels match)
// ---------------------------------------------------------------------------

fn bench_compare_identical(c: &mut Criterion) {
    let mut group = c.benchmark_group("compare_images/identical");
    let opts = CompareOptions::default();

    for &(w, h, label) in &[(512, 256, "512x256"), (1920, 1080, "1920x1080")] {
        let img = make_solid(w, h, 128, 128, 128);
        let pixels = (w as u64) * (h as u64);
        group.throughput(Throughput::Elements(pixels));
        group.bench_with_input(BenchmarkId::from_parameter(label), &img, |b, img| {
            b.iter(|| kaiki_diff::compare_images(img, img, &opts));
        });
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// 6. compare_images/all-diff — worst case (all pixels differ)
// ---------------------------------------------------------------------------

fn bench_compare_all_diff(c: &mut Criterion) {
    let mut group = c.benchmark_group("compare_images/all-diff");
    let opts = CompareOptions::default();

    for &(w, h, label) in &[(512, 256, "512x256"), (1920, 1080, "1920x1080")] {
        let black = make_solid(w, h, 0, 0, 0);
        let white = make_solid(w, h, 255, 255, 255);
        let pixels = (w as u64) * (h as u64);
        group.throughput(Throughput::Elements(pixels));
        group.bench_with_input(BenchmarkId::from_parameter(label), &(), |b, _| {
            b.iter(|| kaiki_diff::compare_images(&black, &white, &opts));
        });
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// 7. compare_images/noisy-1080p — realistic (~25% changed, 1920x1080)
// ---------------------------------------------------------------------------

fn bench_compare_noisy_1080p(c: &mut Criterion) {
    let mut group = c.benchmark_group("compare_images/noisy-1080p");
    let base = make_solid(1920, 1080, 128, 128, 128);
    let noisy = make_noisy(&base);
    let pixels = 1920u64 * 1080;

    group.throughput(Throughput::Elements(pixels));

    // AA detection ON (enable_antialias: false = AA detection ON)
    let opts_aa_on = CompareOptions {
        enable_antialias: false,
        ..CompareOptions::default()
    };
    group.bench_function("aa-on", |b| {
        b.iter(|| kaiki_diff::compare_images(&base, &noisy, &opts_aa_on));
    });

    // AA detection OFF
    let opts_aa_off = CompareOptions {
        enable_antialias: true,
        ..CompareOptions::default()
    };
    group.bench_function("aa-off", |b| {
        b.iter(|| kaiki_diff::compare_images(&base, &noisy, &opts_aa_off));
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// 8. compare_images/antialias — AA ON/OFF with fixture 1a/1b (pre-decoded)
// ---------------------------------------------------------------------------

fn bench_compare_antialias(c: &mut Criterion) {
    let mut group = c.benchmark_group("compare_images/antialias");

    let actual = decode(&fixture("1a.png"));
    let expected = decode(&fixture("1b.png"));
    let pixels = (actual.width as u64) * (actual.height as u64);
    group.throughput(Throughput::Elements(pixels));

    let opts_aa_on = CompareOptions {
        enable_antialias: false,
        ..CompareOptions::default()
    };
    group.bench_function("aa-on", |b| {
        b.iter(|| kaiki_diff::compare_images(&actual, &expected, &opts_aa_on));
    });

    let opts_aa_off = CompareOptions {
        enable_antialias: true,
        ..CompareOptions::default()
    };
    group.bench_function("aa-off", |b| {
        b.iter(|| kaiki_diff::compare_images(&actual, &expected, &opts_aa_off));
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// 9. compare_images/threshold — threshold sensitivity (pre-decoded)
// ---------------------------------------------------------------------------

fn bench_compare_threshold(c: &mut Criterion) {
    let mut group = c.benchmark_group("compare_images/threshold");

    let actual = decode(&fixture("1a.png"));
    let expected = decode(&fixture("1b.png"));
    let pixels = (actual.width as u64) * (actual.height as u64);
    group.throughput(Throughput::Elements(pixels));

    for threshold in [0.05, 0.1, 0.2] {
        let opts = CompareOptions {
            matching_threshold: threshold,
            ..CompareOptions::default()
        };
        group.bench_function(format!("t={threshold}"), |b| {
            b.iter(|| kaiki_diff::compare_images(&actual, &expected, &opts));
        });
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// 10. compare_images/scaling — resolution scaling (pixels/sec consistency)
// ---------------------------------------------------------------------------

fn bench_compare_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("compare_images/scaling");
    let opts = CompareOptions::default();

    for &(w, h) in &[(256, 256), (512, 512), (1024, 1024), (1920, 1080)] {
        let base = make_solid(w, h, 100, 150, 200);
        let noisy = make_noisy(&base);
        let pixels = (w as u64) * (h as u64);
        let label = format!("{w}x{h}");

        group.throughput(Throughput::Elements(pixels));
        group.bench_with_input(BenchmarkId::from_parameter(&label), &(), |b, _| {
            b.iter(|| kaiki_diff::compare_images(&base, &noisy, &opts));
        });
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// Criterion groups
// ---------------------------------------------------------------------------

criterion_group!(
    benches,
    bench_e2e_fixture,
    bench_e2e_hash_fastpath,
    bench_decode,
    bench_compare_fixture,
    bench_compare_identical,
    bench_compare_all_diff,
    bench_compare_noisy_1080p,
    bench_compare_antialias,
    bench_compare_threshold,
    bench_compare_scaling,
);
criterion_main!(benches);
