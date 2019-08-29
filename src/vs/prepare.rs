use std::fs::File;
use std::io::{Error, ErrorKind, Read};
use std::iter::FromIterator;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use local_encoding::{Encoder, Encoding};

use crate::cmd;
use crate::compiler::{
    Arg, CommandInfo, CompilationArgs, CompilationTask, InputKind, OutputKind, Scope,
};

enum ParamValue<T> {
    None,
    Single(T),
    Many(Vec<T>),
}

pub fn create_tasks(command: CommandInfo, args: &[String]) -> Result<Vec<CompilationTask>, String> {
    load_arguments(&command.current_dir, args.iter())
        .map_err(|e: Error| format!("IO error: {}", e))
        .and_then(|a| parse_arguments(a.iter()))
        .and_then(|parsed_args| {
            // Source file name.
            let input_sources: Vec<PathBuf> = parsed_args
                .iter()
                .filter_map(|arg| match arg {
                    Arg::Input {
                        ref kind, ref file, ..
                    } if *kind == InputKind::Source => Some(Path::new(file).to_path_buf()),
                    _ => None,
                })
                .collect();
            if input_sources.is_empty() {
                return Err("Can't find source file path.".to_string());
            }
            // Precompiled header file name.
            let precompiled_file = match find_param(&parsed_args, |arg: &Arg| -> Option<PathBuf> {
                match *arg {
                    Arg::Input {
                        ref kind, ref file, ..
                    } if *kind == InputKind::Precompiled => Some(Path::new(file).to_path_buf()),
                    _ => None,
                }
            }) {
                ParamValue::None => None,
                ParamValue::Single(v) => Some(v),
                ParamValue::Many(v) => {
                    return Err(format!("Found too many precompiled header files: {:?}", v));
                }
            };
            let cwd = command.current_dir.clone();
            // Precompiled header file name.
            let marker_precompiled;
            let input_precompiled;
            let output_precompiled;
            match find_param(&parsed_args, |arg: &Arg| -> Option<(bool, String)> {
                match *arg {
                    Arg::Input {
                        ref kind, ref file, ..
                    } if *kind == InputKind::Marker => Some((true, file.clone())),
                    Arg::Output {
                        ref kind, ref file, ..
                    } if *kind == OutputKind::Marker => Some((false, file.clone())),
                    _ => None,
                }
            }) {
                ParamValue::None => {
                    marker_precompiled = None;
                    input_precompiled = None;
                    output_precompiled = None;
                }
                ParamValue::Single((input, path)) => {
                    let precompiled_path = match precompiled_file {
                        Some(v) => v,
                        None => Path::new(&path).with_extension(".pch").to_path_buf(),
                    };
                    marker_precompiled = if path.is_empty() { None } else { Some(path) };
                    if input {
                        output_precompiled = None;
                        input_precompiled = Some(precompiled_path);
                    } else {
                        input_precompiled = None;
                        output_precompiled = Some(precompiled_path);
                    }
                }
                ParamValue::Many(v) => {
                    return Err(format!(
                        "Found too many precompiled header markers: {}",
                        v.iter().map(|item| item.1.clone()).collect::<String>()
                    ));
                }
            };
            // Output object file name.
            let output_object: Option<PathBuf> =
                match find_param(&parsed_args, |arg: &Arg| -> Option<PathBuf> {
                    match *arg {
                        Arg::Output {
                            ref kind, ref file, ..
                        } if *kind == OutputKind::Object => Some(Path::new(file).to_path_buf()),
                        _ => None,
                    }
                }) {
                    ParamValue::None => None,
                    ParamValue::Single(v) => Some(v),
                    ParamValue::Many(v) => {
                        return Err(format!("Found too many output object files: {:?}", v));
                    }
                }
                .map(|path| cwd.as_ref().map(|cwd| cwd.join(&path)).unwrap_or(path));
            // Language
            let language: Option<String> =
                match find_param(&parsed_args, |arg: &Arg| -> Option<String> {
                    match arg {
                        Arg::Param {
                            ref flag,
                            ref value,
                            ..
                        } if *flag == "T" => Some(value.clone()),
                        _ => None,
                    }
                }) {
                    ParamValue::None => None,
                    ParamValue::Single(v) => Some(v.clone()),
                    ParamValue::Many(v) => {
                        return Err(format!("Found too many output object files: {:?}", v));
                    }
                };
            let shared = Arc::new(CompilationArgs {
                args: parsed_args,
                input_precompiled: input_precompiled.map(|path| command.current_dir_join(&path)),
                output_precompiled: output_precompiled.map(|path| command.current_dir_join(&path)),
                marker_precompiled,
                command,
            });
            input_sources
                .into_iter()
                .map(|source| {
                    let input_source = cwd.as_ref().map(|cwd| cwd.join(&source)).unwrap_or(source);
                    Ok(CompilationTask {
                        shared: shared.clone(),
                        language: language
                            .as_ref()
                            .map_or_else(
                                || {
                                    input_source
                                        .extension()
                                        .and_then(|ext| match ext.to_str() {
                                            Some(e) if e.eq_ignore_ascii_case("cpp") => Some("P"),
                                            Some(e) if e.eq_ignore_ascii_case("c") => Some("C"),
                                            _ => None,
                                        })
                                        .map(|ext| ext.to_string())
                                },
                                |lang| Some(lang.clone()),
                            )
                            .ok_or_else(|| {
                                format!(
                                    "Can't detect file language by extension: {}",
                                    input_source.to_string_lossy()
                                )
                            })?,
                        output_object: get_output_object(&input_source, &output_object)?,
                        input_source,
                    })
                })
                .collect()
        })
}

fn get_output_object(
    input_source: &Path,
    output_object: &Option<PathBuf>,
) -> Result<PathBuf, String> {
    output_object.as_ref().map_or_else(
        || Ok(input_source.with_extension("obj")),
        |path| {
            if path.is_dir() {
                input_source
                    .file_name()
                    .map(|name| path.join(name).with_extension("obj"))
                    .ok_or_else(|| {
                        format!(
                            "Input file path does not contains file name: {}",
                            input_source.to_string_lossy()
                        )
                    })
            } else {
                Ok(path.clone())
            }
        },
    )
}

fn find_param<T, R, F: Fn(&T) -> Option<R>>(args: &[T], filter: F) -> ParamValue<R> {
    let mut found = Vec::from_iter(args.iter().filter_map(filter));
    match found.len() {
        0 => ParamValue::None,
        1 => ParamValue::Single(found.pop().unwrap()),
        _ => ParamValue::Many(found),
    }
}

fn load_arguments<S: AsRef<str>, I: Iterator<Item = S>>(
    base: &Option<PathBuf>,
    iter: I,
) -> Result<Vec<String>, Error> {
    let mut result: Vec<String> = Vec::new();
    for item in iter {
        if item.as_ref().starts_with('@') {
            let path = match base {
                Some(ref p) => p.join(&item.as_ref()[1..]),
                None => Path::new(&item.as_ref()[1..]).to_path_buf(),
            };
            let mut file = File::open(path)?;
            let mut data = Vec::new();
            file.read_to_end(&mut data)?;
            let text = decode_string(&data)?;
            let mut args = cmd::native::parse(&text)?;
            result.append(&mut args);
        } else {
            result.push(item.as_ref().to_string());
        }
    }
    Ok(result)
}

fn decode_string(data: &[u8]) -> Result<String, Error> {
    if data.starts_with(&[0xEF, 0xBB, 0xBF]) {
        String::from_utf8(data[3..].to_vec()).map_err(|e| Error::new(ErrorKind::InvalidInput, e))
    } else if data.starts_with(&[0xFE, 0xFF]) {
        decode_utf16(&data[2..], |a, b| (a << 8) + b)
    } else if data.starts_with(&[0xFF, 0xFE]) {
        decode_utf16(&data[2..], |a, b| (b << 8) + a)
    } else {
        Encoding::ANSI.to_string(data)
    }
}

fn decode_utf16<F: Fn(u16, u16) -> u16>(data: &[u8], endian: F) -> Result<String, Error> {
    let mut utf16 = Vec::new();
    if data.len() % 2 != 0 {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "Invalid UTF-16 line: odd bytes length",
        ));
    }
    let mut i = 0;
    while i < data.len() {
        utf16.push(endian(u16::from(data[i]), u16::from(data[i + 1])));
        i += 2;
    }
    String::from_utf16(&utf16).map_err(|e| Error::new(ErrorKind::InvalidInput, e))
}

fn parse_arguments<S: AsRef<str>, I: Iterator<Item = S>>(mut iter: I) -> Result<Vec<Arg>, String> {
    let mut result: Vec<Arg> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    while let Some(parse_result) = parse_argument(&mut iter) {
        match parse_result {
            Ok(arg) => {
                result.push(arg);
            }
            Err(e) => {
                errors.push(e);
            }
        }
    }
    if !errors.is_empty() {
        return Err(format!(
            "Found unknown command line arguments: {:?}",
            errors
        ));
    }
    Ok(result)
}

fn parse_argument<S: AsRef<str>, I: Iterator<Item = S>>(
    iter: &mut I,
) -> Option<Result<Arg, String>> {
    match iter.next() {
        Some(arg) => Some(if has_param_prefix(arg.as_ref()) {
            let flag = &arg.as_ref()[1..];
            match is_spaceable_param(flag) {
                Some((prefix, scope)) => {
                    if flag == prefix {
                        match iter.next() {
                            Some(value) => {
                                if !has_param_prefix(value.as_ref()) {
                                    Ok(Arg::param(scope, prefix, value.as_ref()))
                                } else {
                                    Err(arg.as_ref().to_string())
                                }
                            }
                            _ => Err(arg.as_ref().to_string()),
                        }
                    } else {
                        Ok(Arg::param(scope, prefix, &flag[prefix.len()..]))
                    }
                }
                None => match flag {
                    "c" | "nologo" => Ok(Arg::flag(Scope::Ignore, flag)),
                    "bigobj" => Ok(Arg::flag(Scope::Compiler, flag)),
                    s if s.starts_with('T') => Ok(Arg::param(Scope::Ignore, "T", &s[1..])),
                    s if s.starts_with('O') => Ok(Arg::flag(Scope::Shared, flag)),
                    s if s.starts_with('G') => Ok(Arg::flag(Scope::Shared, flag)),
                    s if s.starts_with("RTC") => Ok(Arg::flag(Scope::Shared, flag)),
                    s if s.starts_with('Z') => Ok(Arg::flag(Scope::Shared, flag)),
                    s if s.starts_with("d2Zi+") => Ok(Arg::flag(Scope::Shared, flag)),
                    s if s.starts_with("MP") => Ok(Arg::flag(Scope::Compiler, flag)),
                    s if s.starts_with("MD") => Ok(Arg::flag(Scope::Shared, flag)),
                    s if s.starts_with("MT") => Ok(Arg::flag(Scope::Shared, flag)),
                    s if s.starts_with("EH") => Ok(Arg::flag(Scope::Shared, flag)),
                    s if s.starts_with("fp:") => Ok(Arg::flag(Scope::Shared, flag)),
                    s if s.starts_with("arch:") => Ok(Arg::flag(Scope::Shared, flag)),
                    s if s.starts_with("errorReport:") => Ok(Arg::flag(Scope::Shared, flag)),
                    s if s.starts_with("Fo") => Ok(Arg::output(OutputKind::Object, "Fo", &s[2..])),
                    s if s.starts_with("Fp") => {
                        Ok(Arg::input(InputKind::Precompiled, "Fp", &s[2..]))
                    }
                    s if s.starts_with("Yc") => Ok(Arg::output(OutputKind::Marker, "Yc", &s[2..])),
                    s if s.starts_with("Yu") => Ok(Arg::input(InputKind::Marker, "Yu", &s[2..])),
                    s if s.starts_with("Yl") => Ok(Arg::flag(Scope::Shared, flag)),
                    s if s.starts_with("FI") => Ok(Arg::param(Scope::Preprocessor, "FI", &s[2..])),
                    s if s.starts_with("analyze") => Ok(Arg::flag(Scope::Shared, flag)),
                    _ => Err(arg.as_ref().to_string()),
                },
            }
        } else {
            Ok(Arg::Input {
                kind: InputKind::Source,
                flag: String::new(),
                file: arg.as_ref().to_string(),
            })
        }),
        None => None,
    }
}

fn is_spaceable_param(flag: &str) -> Option<(&str, Scope)> {
    for prefix in ["D"].iter() {
        if flag.starts_with(*prefix) {
            return Some((*prefix, Scope::Shared));
        }
    }
    for prefix in ["I"].iter() {
        if flag.starts_with(*prefix) {
            return Some((*prefix, Scope::Preprocessor));
        }
    }
    for prefix in ["W", "wd", "we", "wo", "w"].iter() {
        if flag.starts_with(*prefix) {
            return Some((*prefix, Scope::Compiler));
        }
    }
    None
}

fn has_param_prefix(arg: &str) -> bool {
    arg.starts_with('/') || arg.starts_with('-')
}

#[test]
fn test_parse_argument() {
    let args = Vec::from_iter(
        "/TP /c /Yusample.h /Fpsample.h.pch /Fosample.cpp.o /DTEST /D TEST2 /arch:AVX \
         sample.cpp"
            .split(' ')
            .map(|x| x.to_string()),
    );
    assert_eq!(
        parse_arguments(args.iter()).unwrap(),
        [
            Arg::param(Scope::Ignore, "T", "P"),
            Arg::flag(Scope::Ignore, "c"),
            Arg::input(InputKind::Marker, "Yu", "sample.h"),
            Arg::input(InputKind::Precompiled, "Fp", "sample.h.pch"),
            Arg::output(OutputKind::Object, "Fo", "sample.cpp.o"),
            Arg::param(Scope::Shared, "D", "TEST"),
            Arg::param(Scope::Shared, "D", "TEST2"),
            Arg::flag(Scope::Shared, "arch:AVX"),
            Arg::input(InputKind::Source, "", "sample.cpp")
        ]
    )
}

#[test]
fn test_decode_string() {
    // ANSI
    assert_eq!(&decode_string(b"test").unwrap(), "test");
    // UTF-8
    assert_eq!(
        &decode_string(b"\xEF\xBB\xBFtest \xD1\x80\xD1\x83\xD1\x81").unwrap(),
        "test рус"
    );
    // UTF-16LE
    assert_eq!(
        &decode_string(b"\xFF\xFEt\x00e\x00s\x00t\x00 \x00\x40\x04\x43\x04\x41\x04").unwrap(),
        "test рус"
    );
    // UTF-16BE
    assert_eq!(
        &decode_string(b"\xFE\xFF\x00t\x00e\x00s\x00t\x00 \x04\x40\x04\x43\x04\x41").unwrap(),
        "test рус"
    );
}
