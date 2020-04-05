use std::fs;
use std::io::Cursor;

use criterion::{criterion_group, criterion_main, Criterion};

use octobuild::vs::postprocess;

fn filter_preprocessed_benchmark(c: &mut Criterion) {
    c.bench_function("filter_preprocessed", |b| {
        let source = fs::read("tests/filter_preprocessed.i").unwrap();
        let marker = Some("c:\\bozaro\\github\\octobuild\\test_cl\\sample.h".to_string());

        b.iter(|| {
            let mut result = Vec::with_capacity(source.len());
            postprocess::filter_preprocessed(
                &mut Cursor::new(source.clone()),
                &mut result,
                &marker,
                false,
            )
            .unwrap();
            result
        })
    });
}

criterion_group!(benches, filter_preprocessed_benchmark);
criterion_main!(benches);
