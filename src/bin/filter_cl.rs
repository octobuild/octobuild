use std::fs;
use std::io::Cursor;

use clap::Parser;

use octobuild::vs::postprocess;

fn bench_filter(path: &str, marker: &Option<String>, keep_headers: bool, num: usize) -> Vec<u8> {
    let source = fs::read(path).unwrap();

    let mut total: usize = 0;
    let mut result = Vec::with_capacity(source.len());
    for _ in 0..num {
        result.clear();
        postprocess::filter_preprocessed(
            &mut Cursor::new(source.clone()),
            &mut result,
            marker,
            keep_headers,
        )
        .unwrap();
        total += result.len();
    }
    assert_eq!(total / num, result.len());
    result
}

/// Preprocessor filter for CL.exe compiler test tool
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Precompiled header marker (like StdAfx.h)
    #[arg(long, short)]
    marker: Option<String>,

    /// Keep header before precompiled header marker
    #[arg(long, short)]
    keep: bool,

    /// Number of iterations
    #[arg(short, long, default_value_t = 1)]
    count: usize,

    /// Preprocessed input file
    #[arg()]
    input: String,
}

fn main() {
    let args = Args::parse();

    bench_filter(&args.input, &args.marker, args.keep, args.count);
}
