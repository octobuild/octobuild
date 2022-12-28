use std::io::Error;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::compiler::{
    Arg, CommandInfo, CompilationArgs, CompilationTask, InputKind, OutputKind, Scope,
};
use crate::utils::{expand_response_files, find_param, ParamValue};

pub fn create_tasks(command: CommandInfo, args: &[String]) -> Result<Vec<CompilationTask>, String> {
    let expanded_args = expand_response_files(&command.current_dir, args)
        .map_err(|e: Error| format!("IO error: {}", e))?;

    let parsed_args = parse_arguments(expanded_args.iter())?;
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
                None => Path::new(&path).with_extension(".pch"),
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
    let language: Option<String> = match find_param(&parsed_args, |arg: &Arg| -> Option<String> {
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
        ParamValue::Single(v) => Some(v),
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
        deps_file: None,
    });
    input_sources
        .into_iter()
        .map(|source| {
            let input_source = cwd.as_ref().map(|cwd| cwd.join(&source)).unwrap_or(source);
            let language = language
                .as_ref()
                .map_or_else(|| detect_language(&input_source), |lang| Some(lang.clone()))
                .ok_or_else(|| {
                    format!(
                        "Can't detect file language by extension: {}",
                        input_source.to_string_lossy()
                    )
                })?;
            Ok(CompilationTask {
                shared: shared.clone(),
                language,
                output_object: get_output_object(&input_source, &output_object)?,
                input_source,
            })
        })
        .collect()
}

fn detect_language(path: &Path) -> Option<String> {
    println!("{}", path.to_string_lossy());
    let ext = path.extension()?.to_str()?;
    if ext.eq_ignore_ascii_case("cpp") || ext.eq_ignore_ascii_case("cc") {
        Some("P".to_string())
    } else if ext.eq_ignore_ascii_case("c") {
        Some("C".to_string())
    } else {
        None
    }
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

#[allow(clippy::cognitive_complexity)]
fn parse_argument<S: AsRef<str>, I: Iterator<Item = S>>(
    iter: &mut I,
) -> Option<Result<Arg, String>> {
    iter.next().map(|arg| {
        if has_param_prefix(arg.as_ref()) {
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
                    "FC" => Ok(Arg::flag(Scope::Shared, flag)),
                    s if s.starts_with('T') => Ok(Arg::param(Scope::Ignore, "T", &s[1..])),
                    s if s.starts_with('O') => Ok(Arg::flag(Scope::Shared, flag)),
                    s if s.starts_with('G') => Ok(Arg::flag(Scope::Shared, flag)),
                    s if s.starts_with("RTC") => Ok(Arg::flag(Scope::Shared, flag)),
                    s if s.starts_with('Z') => Ok(Arg::flag(Scope::Shared, flag)),
                    s if s.starts_with("d2Zi+") => Ok(Arg::flag(Scope::Shared, flag)),
                    s if s.starts_with("std:") => Ok(Arg::flag(Scope::Shared, flag)),
                    s if s.starts_with("MP") => Ok(Arg::flag(Scope::Compiler, flag)),
                    s if s.starts_with("MD") => Ok(Arg::flag(Scope::Shared, flag)),
                    s if s.starts_with("MT") => Ok(Arg::flag(Scope::Shared, flag)),
                    s if s.starts_with("EH") => Ok(Arg::flag(Scope::Shared, flag)),
                    s if s.starts_with("fp:") => Ok(Arg::flag(Scope::Shared, flag)),
                    s if s.starts_with("arch:") => Ok(Arg::flag(Scope::Shared, flag)),
                    s if s.starts_with("errorReport:") => Ok(Arg::flag(Scope::Shared, flag)),
                    s if s.starts_with("source-charset:") => Ok(Arg::flag(Scope::Shared, flag)),
                    s if s.starts_with("execution-charset:") => Ok(Arg::flag(Scope::Shared, flag)),
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
        }
    })
}

fn is_spaceable_param(flag: &str) -> Option<(&str, Scope)> {
    for prefix in ["D"].iter() {
        if flag.starts_with(*prefix) {
            return Some((*prefix, Scope::Shared));
        }
    }
    for prefix in ["I", "sourceDependencies"].iter() {
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
    let args: Vec<String> =
        "/TP /c /Yusample.h /Fpsample.h.pch /Fosample.cpp.o /DTEST /D TEST2 /arch:AVX \
         sample.cpp"
            .split(' ')
            .map(|x| x.to_string())
            .collect();
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
