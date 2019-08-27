use std::fs::File;
use std::io::{Cursor, Read};

use self::octobuild::vs::postprocess;
use self::test::Bencher;

fn bench_filter(b: &mut Bencher, path: &str, marker: Option<String>, keep_headers: bool) {
    let mut source = Vec::new();
    File::open(path).unwrap().read_to_end(&mut source).unwrap();
    b.iter(|| {
        let mut result = Vec::with_capacity(source.len());
        postprocess::filter_preprocessed(&mut Cursor::new(source.clone()), &mut result, &marker, keep_headers).unwrap();
        result
    });
}

#[bench]
fn bench_check_filter(b: &mut Bencher) {
    bench_filter(
        b,
        "tests/filter_preprocessed.i",
        Some("c:\\bozaro\\github\\octobuild\\test_cl\\sample.h".to_string()),
        false,
    )
}
