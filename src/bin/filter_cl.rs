use std::fs;
use std::io::Cursor;

use clap::{Arg, Command};

use octobuild::version::{AUTHORS, VERSION};
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

fn main() {
    const MARKER: &str = "marker";
    const INPUT: &str = "input";
    const COUNT: &str = "count";
    const KEEP: &str = "keep";

    let matches = Command::new("filter_cl")
        .arg_required_else_help(true)
        .version(VERSION)
        .author(AUTHORS)
        .about("Preprocessor filter for CL.exe compiler test tool")
        .arg(
            Arg::new(MARKER)
                .short('m')
                .long("marker")
                .value_name("header")
                .takes_value(true)
                .help("Precompiled header marker (like StdAfx.h)"),
        )
        .arg(
            Arg::new(KEEP)
                .short('k')
                .long("keep")
                .help("Keep header before precompiled header marker"),
        )
        .arg(
            Arg::new(COUNT)
                .short('c')
                .long("count")
                .default_value("1")
                .help("Iteration count"),
        )
        .arg(
            Arg::new(INPUT)
                .required(true)
                .index(1)
                .help("Preprocessed input file"),
        )
        .get_matches();

    let inputs = matches.values_of_lossy(INPUT).unwrap();
    let marker = matches.value_of(MARKER).map(|s| s.to_string());
    let keep = matches.is_present(KEEP);
    let count = matches
        .value_of(COUNT)
        .unwrap_or("1")
        .parse::<usize>()
        .unwrap();

    for input in inputs.iter() {
        bench_filter(input, &marker, keep, count);
    }
}
