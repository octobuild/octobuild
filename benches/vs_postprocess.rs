use std::ffi::OsString;
use std::fs;
use std::io::Cursor;

use criterion::{criterion_group, criterion_main, Criterion};

use octobuild::vs::postprocess;
use std::path::PathBuf;

fn filter_preprocessed_benchmark(c: &mut Criterion) {
    c.bench_function("filter_preprocessed", |b| {
        let f = PathBuf::from(file!())
            .parent()
            .unwrap()
            .join(PathBuf::from("filter_preprocessed.i"));
        let source = fs::read(f).unwrap();
        let marker = Some(OsString::from(
            "c:\\bozaro\\github\\octobuild\\test_cl\\sample.h",
        ));

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
