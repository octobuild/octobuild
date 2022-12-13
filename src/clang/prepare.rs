use std::path::{Path, PathBuf};
use std::slice::Iter;
use std::sync::Arc;

use crate::compiler::{
    Arg, CommandInfo, CompilationArgs, CompilationTask, InputKind, OutputKind, Scope,
};
use crate::utils::expands_response_files;

enum ParamValue<T> {
    None,
    Single(T),
    Many(Vec<T>),
}

pub fn create_tasks(command: CommandInfo, args: &[String]) -> Result<Vec<CompilationTask>, String> {
    let expanded_args =
        expands_response_files(&command.current_dir, args).map_err(|e| e.to_string())?;

    if expanded_args.iter().any(|v| v == "--analyze") {
        // Support only compilation steps
        return Ok(Vec::new());
    }

    if !expanded_args.iter().any(|v| matches!(v as &str, "-c")) {
        // Support only compilation steps
        return Ok(Vec::new());
    }

    let parsed_args = parse_arguments(&expanded_args)?;
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
    let input_precompiled = match find_param(&parsed_args, |arg: &Arg| -> Option<PathBuf> {
        match arg {
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
    // Precompiled header file name.
    let marker_precompiled = parsed_args
        .iter()
        .filter_map(|arg| match arg {
            Arg::Param {
                ref flag,
                ref value,
                ..
            } if *flag == "include" => Some(value.clone()),
            _ => None,
        })
        .next();
    // Output object file name.
    let output_object = match find_param(&parsed_args, |arg: &Arg| -> Option<PathBuf> {
        match arg {
            Arg::Output {
                ref kind, ref file, ..
            } if *kind == OutputKind::Object => Some(Path::new(file).to_path_buf()),
            _ => None,
        }
    }) {
        ParamValue::None => None,
        ParamValue::Single(v) => {
            if input_sources.len() > 1 {
                return Err("Cannot specify -o when generating multiple output files".to_string());
            }
            Some(v)
        }
        ParamValue::Many(v) => {
            return Err(format!("Found too many output object files: {:?}", v));
        }
    };
    // Language
    let language: Option<String> = match find_param(&parsed_args, |arg: &Arg| -> Option<String> {
        match arg {
            Arg::Param {
                ref flag,
                ref value,
                ..
            } if *flag == "x" => Some(value.clone()),
            _ => None,
        }
    }) {
        ParamValue::None => None,
        ParamValue::Single(v) => {
            match &v[..] {
                "c" | "c++" => Some(v.to_string()),
                "c-header" | "c++-header" => {
                    // Precompiled headers must build locally
                    return Ok(Vec::new());
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
    let shared = Arc::new(CompilationArgs {
        command,
        args: parsed_args,
        output_precompiled: None,
        marker_precompiled,
        input_precompiled,
    });
    input_sources
        .into_iter()
        .map(|source| {
            Ok(CompilationTask {
                shared: shared.clone(),
                language: language
                    .as_ref()
                    .map_or_else(
                        || {
                            source
                                .extension()
                                .and_then(|ext| match ext.to_str() {
                                    Some(e) if e.eq_ignore_ascii_case("cpp") => Some("c++"),
                                    Some(e) if e.eq_ignore_ascii_case("c") => Some("c"),
                                    Some(e) if e.eq_ignore_ascii_case("hpp") => Some("c++-header"),
                                    Some(e) if e.eq_ignore_ascii_case("h") => Some("c-header"),
                                    _ => None,
                                })
                                .map(|ext| ext.to_string())
                        },
                        |lang| Some(lang.clone()),
                    )
                    .ok_or_else(|| {
                        format!(
                            "Can't detect file language by extension: {}",
                            source.as_os_str().to_string_lossy()
                        )
                    })?,
                output_object: output_object
                    .as_ref()
                    .map_or_else(|| source.with_extension("o"), |path| path.clone()),
                input_source: source,
            })
        })
        .collect()
}

fn find_param<T, R, F: Fn(&T) -> Option<R>>(args: &[T], filter: F) -> ParamValue<R> {
    let mut found: Vec<R> = args.iter().filter_map(filter).collect();
    match found.len() {
        0 => ParamValue::None,
        1 => ParamValue::Single(found.pop().unwrap()),
        _ => ParamValue::Many(found),
    }
}

fn parse_arguments(args: &[String]) -> Result<Vec<Arg>, String> {
    let mut result: Vec<Arg> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    let mut iter = args.iter();
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

fn parse_argument(iter: &mut Iter<String>) -> Option<Result<Arg, String>> {
    match iter.next() {
        Some(arg) => Some(if arg.starts_with("--") {
            let (key, value) = match arg.find('=') {
                Some(position) => (&arg[1..position], arg[position + 1..].to_string()),
                None => match iter.next() {
                    Some(v) => (&arg[1..], v.clone()),
                    _ => {
                        return Some(Err(arg.to_string()));
                    }
                },
            };
            match &key[1..] {
                "sysroot" => Ok(Arg::flag(Scope::Shared, key.to_string() + "=" + &value)),
                _ => Err(key.to_string()),
            }
        } else if has_param_prefix(arg) {
            let flag = &arg[1..];
            match is_spaceable_param(flag) {
                Some((prefix, scope, next_flag)) => {
                    let value = if flag == prefix {
                        match iter.next() {
                            Some(v) if next_flag == has_param_prefix(v) => v.to_string(),
                            _ => {
                                return Some(Err(arg.to_string()));
                            }
                        }
                    } else {
                        flag[prefix.len()..].to_string()
                    };
                    match flag {
                        "o" => Ok(Arg::output(OutputKind::Object, prefix, value)),
                        _ => Ok(Arg::param(scope, prefix, value)),
                    }
                }
                None => match flag {
                    "c" => Ok(Arg::flag(Scope::Ignore, flag)),
                    "pipe" => Ok(Arg::flag(Scope::Shared, flag)),
                    "nostdinc++" => Ok(Arg::flag(Scope::Shared, flag)),
                    s if s.starts_with('f') => Ok(Arg::flag(Scope::Shared, flag)),
                    s if s.starts_with('g') => Ok(Arg::flag(Scope::Shared, flag)),
                    s if s.starts_with('O') => Ok(Arg::flag(Scope::Shared, flag)),
                    s if s.starts_with('W') => Ok(Arg::flag(Scope::Shared, flag)),
                    s if s.starts_with('m') => Ok(Arg::flag(Scope::Shared, flag)),
                    s if s.starts_with("std=") => Ok(Arg::flag(Scope::Shared, flag)),
                    _ => Err(arg.to_string()),
                },
            }
        } else {
            Ok(Arg::input(
                InputKind::Source,
                String::new(),
                arg.to_string(),
            ))
        }),
        None => None,
    }
}

fn is_spaceable_param(flag: &str) -> Option<(&str, Scope, bool)> {
    match flag {
        "include" | "include-pch" => Some((flag, Scope::Preprocessor, false)),
        "target" => Some((flag, Scope::Shared, false)),
        _ => {
            for prefix in ["D", "o"].iter() {
                if flag.starts_with(*prefix) {
                    return Some((*prefix, Scope::Shared, false));
                }
            }
            for prefix in ["x"].iter() {
                if flag.starts_with(*prefix) {
                    return Some((*prefix, Scope::Ignore, false));
                }
            }
            for prefix in ["I"].iter() {
                if flag.starts_with(*prefix) {
                    return Some((*prefix, Scope::Preprocessor, false));
                }
            }
            None
        }
    }
}

fn has_param_prefix(arg: &str) -> bool {
    arg.starts_with('-')
}

#[test]
fn test_parse_argument_precompile() {
    let args: Vec<String> =
        "-x c++-header -pipe -Wall -Werror -funwind-tables -Wsequence-point -mmmx -msse -msse2 \
         -fno-math-errno -fno-rtti -g3 -gdwarf-3 -O2 -D_LINUX64 -IEngine/Source \
         -IDeveloper/Public -I Runtime/Core/Private -D IS_PROGRAM=1 -D UNICODE \
         -DIS_MONOLITHIC=1 -std=c++11 -o CorePrivatePCH.h.pch CorePrivatePCH.h"
            .split(' ')
            .map(|x| x.to_string())
            .collect();
    assert_eq!(
        parse_arguments(&args).unwrap(),
        [
            Arg::param(Scope::Ignore, "x", "c++-header"),
            Arg::flag(Scope::Shared, "pipe"),
            Arg::flag(Scope::Shared, "Wall"),
            Arg::flag(Scope::Shared, "Werror"),
            Arg::flag(Scope::Shared, "funwind-tables"),
            Arg::flag(Scope::Shared, "Wsequence-point"),
            Arg::flag(Scope::Shared, "mmmx"),
            Arg::flag(Scope::Shared, "msse"),
            Arg::flag(Scope::Shared, "msse2"),
            Arg::flag(Scope::Shared, "fno-math-errno"),
            Arg::flag(Scope::Shared, "fno-rtti"),
            Arg::flag(Scope::Shared, "g3"),
            Arg::flag(Scope::Shared, "gdwarf-3"),
            Arg::flag(Scope::Shared, "O2"),
            Arg::param(Scope::Shared, "D", "_LINUX64"),
            Arg::param(Scope::Preprocessor, "I", "Engine/Source"),
            Arg::param(Scope::Preprocessor, "I", "Developer/Public"),
            Arg::param(Scope::Preprocessor, "I", "Runtime/Core/Private"),
            Arg::param(Scope::Shared, "D", "IS_PROGRAM=1"),
            Arg::param(Scope::Shared, "D", "UNICODE"),
            Arg::param(Scope::Shared, "D", "IS_MONOLITHIC=1"),
            Arg::flag(Scope::Shared, "std=c++11"),
            Arg::output(OutputKind::Object, "o", "CorePrivatePCH.h.pch"),
            Arg::input(InputKind::Source, "", "CorePrivatePCH.h")
        ]
    )
}

#[test]
fn test_parse_argument_compile() {
    let args: Vec<String> =
        "-c -include-pch CorePrivatePCH.h.pch -pipe -Wall -Werror -funwind-tables \
         -Wsequence-point -mmmx -msse -msse2 -fno-math-errno -fno-rtti -g3 -gdwarf-3 -O2 -D \
         IS_PROGRAM=1 -D UNICODE -DIS_MONOLITHIC=1 -x c++ -std=c++11 -include CorePrivatePCH.h \
         -o Module.Core.cpp.o Module.Core.cpp"
            .split(' ')
            .map(|x| x.to_string())
            .collect();
    assert_eq!(
        parse_arguments(&args).unwrap(),
        [
            Arg::flag(Scope::Ignore, "c"),
            Arg::param(Scope::Preprocessor, "include-pch", "CorePrivatePCH.h.pch"),
            Arg::flag(Scope::Shared, "pipe"),
            Arg::flag(Scope::Shared, "Wall"),
            Arg::flag(Scope::Shared, "Werror"),
            Arg::flag(Scope::Shared, "funwind-tables"),
            Arg::flag(Scope::Shared, "Wsequence-point"),
            Arg::flag(Scope::Shared, "mmmx"),
            Arg::flag(Scope::Shared, "msse"),
            Arg::flag(Scope::Shared, "msse2"),
            Arg::flag(Scope::Shared, "fno-math-errno"),
            Arg::flag(Scope::Shared, "fno-rtti"),
            Arg::flag(Scope::Shared, "g3"),
            Arg::flag(Scope::Shared, "gdwarf-3"),
            Arg::flag(Scope::Shared, "O2"),
            Arg::param(Scope::Shared, "D", "IS_PROGRAM=1"),
            Arg::param(Scope::Shared, "D", "UNICODE"),
            Arg::param(Scope::Shared, "D", "IS_MONOLITHIC=1"),
            Arg::param(Scope::Ignore, "x", "c++"),
            Arg::flag(Scope::Shared, "std=c++11"),
            Arg::param(Scope::Preprocessor, "include", "CorePrivatePCH.h"),
            Arg::output(OutputKind::Object, "o", "Module.Core.cpp.o"),
            Arg::input(InputKind::Source, "", "Module.Core.cpp")
        ]
    )
}
