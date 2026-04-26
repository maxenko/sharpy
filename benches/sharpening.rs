use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use image::RgbImage;
use sharpy::{EdgeMethod, Image, SharpeningPresets};

fn create_test_image(size: u32) -> Image {
    let mut img = RgbImage::new(size, size);

    // Create a pattern with various frequencies
    for y in 0..size {
        for x in 0..size {
            let value = ((x as f32 * 0.1).sin() * 127.0 + 128.0) as u8;
            img.put_pixel(x, y, image::Rgb([value, value, value]));
        }
    }

    Image::from_rgb(img).unwrap()
}

fn benchmark_unsharp_mask(c: &mut Criterion) {
    let mut group = c.benchmark_group("unsharp_mask");

    for size in [256, 512, 1024].iter() {
        let img = create_test_image(*size);

        group.bench_with_input(BenchmarkId::new("size", size), size, |b, _| {
            b.iter(|| {
                let img_clone = img.clone();
                black_box(img_clone.unsharp_mask(1.0, 1.0, 0).unwrap())
            });
        });
    }

    group.finish();
}

fn benchmark_high_pass_sharpen(c: &mut Criterion) {
    let mut group = c.benchmark_group("high_pass_sharpen");

    for size in [256, 512, 1024].iter() {
        let img = create_test_image(*size);

        group.bench_with_input(BenchmarkId::new("size", size), size, |b, _| {
            b.iter(|| {
                let img_clone = img.clone();
                black_box(img_clone.high_pass_sharpen(0.5).unwrap())
            });
        });
    }

    group.finish();
}

fn benchmark_edge_enhancement(c: &mut Criterion) {
    let mut group = c.benchmark_group("edge_enhancement");

    for size in [256, 512, 1024].iter() {
        let img = create_test_image(*size);

        group.bench_with_input(BenchmarkId::new("sobel", size), size, |b, _| {
            b.iter(|| {
                let img_clone = img.clone();
                black_box(img_clone.enhance_edges(1.0, EdgeMethod::Sobel).unwrap())
            });
        });

        group.bench_with_input(BenchmarkId::new("prewitt", size), size, |b, _| {
            b.iter(|| {
                let img_clone = img.clone();
                black_box(img_clone.enhance_edges(1.0, EdgeMethod::Prewitt).unwrap())
            });
        });
    }

    group.finish();
}

fn benchmark_clarity(c: &mut Criterion) {
    let mut group = c.benchmark_group("clarity");

    for size in [256, 512, 1024].iter() {
        let img = create_test_image(*size);

        group.bench_with_input(BenchmarkId::new("size", size), size, |b, _| {
            b.iter(|| {
                let img_clone = img.clone();
                black_box(img_clone.clarity(1.0, 2.0).unwrap())
            });
        });
    }

    group.finish();
}

fn benchmark_builder_pattern(c: &mut Criterion) {
    let mut group = c.benchmark_group("builder_pattern");

    let img = create_test_image(512);

    group.bench_function("single_operation", |b| {
        b.iter(|| {
            let img_clone = img.clone();
            black_box(
                img_clone
                    .sharpen()
                    .unsharp_mask(1.0, 1.0, 0)
                    .apply()
                    .unwrap(),
            )
        });
    });

    group.bench_function("multiple_operations", |b| {
        b.iter(|| {
            let img_clone = img.clone();
            black_box(
                img_clone
                    .sharpen()
                    .unsharp_mask(1.0, 1.0, 0)
                    .high_pass(0.3)
                    .clarity(0.5, 2.0)
                    .apply()
                    .unwrap(),
            )
        });
    });

    group.finish();
}

fn benchmark_presets(c: &mut Criterion) {
    let mut group = c.benchmark_group("presets");

    let img = create_test_image(512);

    group.bench_function("subtle", |b| {
        b.iter(|| {
            let img_clone = img.clone();
            black_box(SharpeningPresets::subtle(img_clone).apply().unwrap())
        });
    });

    group.bench_function("moderate", |b| {
        b.iter(|| {
            let img_clone = img.clone();
            black_box(SharpeningPresets::moderate(img_clone).apply().unwrap())
        });
    });

    group.bench_function("strong", |b| {
        b.iter(|| {
            let img_clone = img.clone();
            black_box(SharpeningPresets::strong(img_clone).apply().unwrap())
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    benchmark_unsharp_mask,
    benchmark_high_pass_sharpen,
    benchmark_edge_enhancement,
    benchmark_clarity,
    benchmark_builder_pattern,
    benchmark_presets
);
criterion_main!(benches);
