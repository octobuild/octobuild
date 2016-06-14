use std::iter::FromIterator;
use std::ascii::AsciiExt;
use std::fs::File;
use std::io::{Error, Read};
use std::path::{Path, PathBuf};

use super::super::cmd;
use super::super::compiler::{Arg, CommandInfo, CompilationTask, InputKind, OutputKind, Scope};
use super::super::utils::filter;

enum ParamValue<T> {
    None,
    Single(T),
    Many(Vec<T>),
}

pub fn create_task(command: CommandInfo, args: &[String]) -> Result<Option<CompilationTask>, String> {
    load_arguments(&command.current_dir, args.iter())
        .map_err(|e: Error| format!("IO error: {}", e))
        .and_then(|a| parse_arguments(a.iter()))
        .and_then(|parsed_args| {
            // Source file name.
            let input_source = match find_param(&parsed_args, |arg: &Arg| -> Option<PathBuf> {
                match *arg {
                    Arg::Input { ref kind, ref file, .. } if *kind == InputKind::Source => {
                        Some(Path::new(file).to_path_buf())
                    }
                    _ => None,
                }
            }) {
                ParamValue::None => {
                    return Err(format!("Can't find source file path."));
                }
                ParamValue::Single(v) => v,
                ParamValue::Many(v) => {
                    return Err(format!("Found too many source files: {:?}", v));
                }
            };
            // Precompiled header file name.
            let precompiled_file = match find_param(&parsed_args, |arg: &Arg| -> Option<PathBuf> {
                match *arg {
                    Arg::Input { ref kind, ref file, .. } if *kind == InputKind::Precompiled => {
                        Some(Path::new(file).to_path_buf())
                    }
                    _ => None,
                }
            }) {
                ParamValue::None => None,
                ParamValue::Single(v) => Some(v),
                ParamValue::Many(v) => {
                    return Err(format!("Found too many precompiled header files: {:?}", v));
                }
            };
            // Precompiled header file name.
            let marker_precompiled;
            let input_precompiled;
            let output_precompiled;
            match find_param(&parsed_args, |arg: &Arg| -> Option<(bool, String)> {
                match *arg {
                    Arg::Input { ref kind, ref file, .. } if *kind == InputKind::Marker => Some((true, file.clone())),
                    Arg::Output { ref kind, ref file, .. } if *kind == OutputKind::Marker => {
                        Some((false, file.clone()))
                    }
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
                    marker_precompiled = if path.len() > 0 {
                        Some(path)
                    } else {
                        None
                    };
                    if input {
                        output_precompiled = None;
                        input_precompiled = Some(precompiled_path);
                    } else {
                        input_precompiled = None;
                        output_precompiled = Some(precompiled_path);
                    }
                }
                ParamValue::Many(v) => {
                    return Err(format!("Found too many precompiled header markers: {}",
                                       v.iter().map(|item| item.1.clone()).collect::<String>()));
                }
            };
            // Output object file name.
            let output_object = match find_param(&parsed_args, |arg: &Arg| -> Option<PathBuf> {
                match *arg {
                    Arg::Output { ref kind, ref file, .. } if *kind == OutputKind::Object => {
                        Some(Path::new(file).to_path_buf())
                    }
                    _ => None,
                }
            }) {
                ParamValue::None => input_source.with_extension("obj"),
                ParamValue::Single(v) => v,
                ParamValue::Many(v) => {
                    return Err(format!("Found too many output object files: {:?}", v));
                }
            };
            // Language
            let language: String;
            match find_param(&parsed_args, |arg: &Arg| -> Option<String> {
                match arg {
                    &Arg::Param { ref flag, ref value, .. } if *flag == "T" => Some(value.clone()),
                    _ => None,
                }
            }) {
                ParamValue::None => {
                    match input_source.extension() {
                        Some(extension) => {
                            match extension.to_str() {
                                Some(e) if e.eq_ignore_ascii_case("cpp") => {
                                    language = "P".to_string();
                                }
                                Some(e) if e.eq_ignore_ascii_case("c") => {
                                    language = "C".to_string();
                                }
                                _ => {
                                    return Err(format!("Can't detect file language by extension: {}",
                                                       input_source.as_os_str().to_string_lossy()));
                                }
                            }
                        }
                        _ => {
                            return Err(format!("Can't detect file language by extension: {:?}",
                                               input_source.as_os_str().to_string_lossy()));
                        }
                    }
                }
                ParamValue::Single(v) => {
                    match &v[..] {
                        "P" | "C" => {
                            language = v.clone();
                        }
                        _ => {
                            return Err(format!("Unknown source language type: {}", v));
                        }
                    }
                }
                ParamValue::Many(v) => {
                    return Err(format!("Found too many output object files: {:?}", v));
                }
            };

            Ok(Some(CompilationTask {
                command: command,
                args: parsed_args,
                language: language,
                input_source: input_source,
                input_precompiled: input_precompiled,
                output_object: output_object,
                output_precompiled: output_precompiled,
                marker_precompiled: marker_precompiled,
            }))
        })
}

fn find_param<T, R, F: Fn(&T) -> Option<R>>(args: &Vec<T>, filter: F) -> ParamValue<R> {
    let mut found = Vec::from_iter(args.iter().filter_map(filter));
    match found.len() {
        0 => ParamValue::None,
        1 => ParamValue::Single(found.pop().unwrap()),
        _ => ParamValue::Many(found),
    }
}

fn load_arguments<S: AsRef<str>, I: Iterator<Item = S>>(base: &Option<PathBuf>, iter: I) -> Result<Vec<String>, Error> {
    let mut result: Vec<String> = Vec::new();
    for item in iter {
        if item.as_ref().starts_with("@") {
            let path = match base {
                &Some(ref p) => p.join(&item.as_ref()[1..]),
                &None => Path::new(&item.as_ref()[1..]).to_path_buf(),
            };
            let mut file = try!(File::open(path));
            let mut text = String::new();
            try!(file.read_to_string(&mut text));
            let mut args = try!(cmd::native::parse(&text));
            result.append(&mut args);
        } else {
            result.push(item.as_ref().to_string());
        }
    }
    Ok(result)
}

fn parse_arguments<S: AsRef<str>, I: Iterator<Item = S>>(mut iter: I) -> Result<Vec<Arg>, String> {
    let mut result: Vec<Arg> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    loop {
        match parse_argument(&mut iter) {
            Some(parse_result) => {
                match parse_result {
                    Ok(arg) => {
                        result.push(arg);
                    }
                    Err(e) => {
                        errors.push(e);
                    }
                }
            }
            None => {
                break;
            }
        }
    }
    if errors.len() > 0 {
        return Err(format!("Found unknown command line arguments: {:?}", errors));
    }
    Ok(result)
}

fn parse_argument<S: AsRef<str>, I: Iterator<Item = S>>(iter: &mut I) -> Option<Result<Arg, String>> {
    match iter.next() {
        Some(arg) => {
            Some(if has_param_prefix(arg.as_ref()) {
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
                    None => {
                        match flag {
                            "c" | "nologo" => Ok(Arg::flag(Scope::Ignore, flag)),
                            "bigobj" => Ok(Arg::flag(Scope::Compiler, flag)),
                            s if s.starts_with("T") => Ok(Arg::param(Scope::Ignore, "T", &s[1..])),
                            s if s.starts_with("O") => Ok(Arg::flag(Scope::Shared, flag)),
                            s if s.starts_with("G") => Ok(Arg::flag(Scope::Shared, flag)),
                            s if s.starts_with("RTC") => Ok(Arg::flag(Scope::Shared, flag)),
                            s if s.starts_with("Z") => Ok(Arg::flag(Scope::Shared, flag)),
                            s if s.starts_with("d2Zi+") => Ok(Arg::flag(Scope::Shared, flag)),
                            s if s.starts_with("MD") => Ok(Arg::flag(Scope::Shared, flag)),
                            s if s.starts_with("MT") => Ok(Arg::flag(Scope::Shared, flag)),
                            s if s.starts_with("EH") => Ok(Arg::flag(Scope::Shared, flag)),
                            s if s.starts_with("fp:") => Ok(Arg::flag(Scope::Shared, flag)),
                            s if s.starts_with("arch:") => Ok(Arg::flag(Scope::Shared, flag)),
                            s if s.starts_with("errorReport:") => Ok(Arg::flag(Scope::Shared, flag)),
                            s if s.starts_with("Fo") => Ok(Arg::output(OutputKind::Object, "Fo", &s[2..])),
                            s if s.starts_with("Fp") => Ok(Arg::input(InputKind::Precompiled, "Fp", &s[2..])),
                            s if s.starts_with("Yc") => Ok(Arg::output(OutputKind::Marker, "Yc", &s[2..])),
                            s if s.starts_with("Yu") => Ok(Arg::input(InputKind::Marker, "Yu", &s[2..])),
                            s if s.starts_with("Yl") => Ok(Arg::flag(Scope::Shared, flag)),
                            s if s.starts_with("FI") => Ok(Arg::param(Scope::Preprocessor, "FI", &s[2..])),
                            _ => Err(arg.as_ref().to_string()),
                        }
                    }
                }
            } else {
                Ok(Arg::Input {
                    kind: InputKind::Source,
                    flag: String::new(),
                    file: arg.as_ref().to_string(),
                })
            })
        }
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
    arg.starts_with("/") || arg.starts_with("-")
}


#[test]
fn test_parse_argument() {
    let args = Vec::from_iter("/TP /c /Yusample.h /Fpsample.h.pch /Fosample.cpp.o /DTEST /D TEST2 /arch:AVX \
                               sample.cpp"
        .split(" ")
        .map(|x| x.to_string()));
    assert_eq!(parse_arguments(args.iter()).unwrap(),
               [Arg::param(Scope::Ignore, "T", "P"),
                Arg::flag(Scope::Ignore, "c"),
                Arg::input(InputKind::Marker, "Yu", "sample.h"),
                Arg::input(InputKind::Precompiled, "Fp", "sample.h.pch"),
                Arg::output(OutputKind::Object, "Fo", "sample.cpp.o"),
                Arg::param(Scope::Shared, "D", "TEST"),
                Arg::param(Scope::Shared, "D", "TEST2"),
                Arg::flag(Scope::Shared, "arch:AVX"),
                Arg::input(InputKind::Source, "", "sample.cpp")])
}
