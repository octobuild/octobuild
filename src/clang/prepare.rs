use std::path::PathBuf;
use std::slice::Iter;
use std::sync::Arc;

use crate::compiler::{
    Arg, CommandInfo, CompilationArgs, CompilationTask, InputKind, OutputKind, Scope,
};
use crate::utils::{expand_response_files, find_param, ParamValue};

pub fn create_tasks(command: CommandInfo, args: &[String]) -> crate::Result<Vec<CompilationTask>> {
    let expanded_args = expand_response_files(&command.current_dir, args)?;

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
            Arg::Input { kind, file, .. } if *kind == InputKind::Source => {
                Some(PathBuf::from(file))
            }
            _ => None,
        })
        .collect();
    if input_sources.is_empty() {
        return Err(crate::Error::from("Can't find source file path."));
    }
    // Precompiled header file name.
    let pch_in = match find_param(&parsed_args, |arg: &Arg| -> Option<PathBuf> {
        match arg {
            Arg::Input { kind, file, .. } if *kind == InputKind::Precompiled => {
                Some(PathBuf::from(file))
            }
            _ => None,
        }
    }) {
        ParamValue::None => None,
        ParamValue::Single(v) => Some(v),
        ParamValue::Many(v) => {
            return Err(crate::Error::from(format!(
                "Found too many precompiled header files: {v:?}"
            )));
        }
    };
    // Precompiled header file name.
    let pch_marker = parsed_args.iter().find_map(|arg| match arg {
        Arg::Param { flag, value, .. } if *flag == "include" => Some(value.clone()),
        _ => None,
    });
    // Output object file name.
    let output_object = match find_param(&parsed_args, |arg: &Arg| -> Option<PathBuf> {
        match arg {
            Arg::Output { kind, file, .. } if *kind == OutputKind::Object => {
                Some(PathBuf::from(file))
            }
            _ => None,
        }
    }) {
        ParamValue::None => None,
        ParamValue::Single(v) => {
            if input_sources.len() > 1 {
                return Err(crate::Error::from(
                    "Cannot specify -o when generating multiple output files",
                ));
            }
            Some(v)
        }
        ParamValue::Many(v) => {
            return Err(crate::Error::from(format!(
                "Found too many output object files: {v:?}"
            )));
        }
    };

    let deps_file = parsed_args.iter().find_map(|arg| match arg {
        Arg::Param { flag, value, .. } if *flag == "MF" => Some(PathBuf::from(value)),
        _ => None,
    });

    // Language
    let language: Option<String> = match find_param(&parsed_args, |arg: &Arg| -> Option<String> {
        match arg {
            Arg::Param { flag, value, .. } if *flag == "x" => Some(value.clone()),
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
                    return Err(crate::Error::from(format!(
                        "Unknown source language type: {v}"
                    )));
                }
            }
        }
        ParamValue::Many(v) => {
            return Err(crate::Error::from(format!(
                "Found too many output object files: {v:?}"
            )));
        }
    };
    let shared = Arc::new(CompilationArgs {
        command,
        args: parsed_args,
        pch_out: None,
        pch_marker,
        pch_in,
        deps_file,
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
                            let lang = match source.extension()?.to_str() {
                                Some(e) if e.eq_ignore_ascii_case("cpp") => Some("c++"),
                                Some(e) if e.eq_ignore_ascii_case("c") => Some("c"),
                                Some(e) if e.eq_ignore_ascii_case("hpp") => Some("c++-header"),
                                Some(e) if e.eq_ignore_ascii_case("h") => Some("c-header"),
                                _ => None,
                            };
                            Some(lang?.to_string())
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
        return Err(format!("Found unknown command line arguments: {errors:?}"));
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
                        "D" => Ok(Arg::param(
                            // Workaround for PS4/PS5
                            if value.starts_with("DUMMY_DEFINE") {
                                Scope::Ignore
                            } else {
                                scope
                            },
                            prefix,
                            value,
                            true,
                        )),
                        "o" => Ok(Arg::output(OutputKind::Object, prefix, value)),
                        _ => Ok(Arg::param(scope, prefix, value, true)),
                    }
                }
                None => match flag {
                    "c" => Ok(Arg::flag(Scope::Ignore, flag)),
                    "pipe" | "nostdinc++" => Ok(Arg::flag(Scope::Shared, flag)),
                    "MD" => Ok(Arg::flag(Scope::Preprocessor, flag)),
                    s if s.starts_with('f') => Ok(Arg::flag(Scope::Shared, flag)),
                    s if s.starts_with('g') => Ok(Arg::flag(Scope::Shared, flag)),
                    s if s.starts_with('O') => Ok(Arg::flag(Scope::Shared, flag)),
                    s if s.starts_with('W') => Ok(Arg::flag(Scope::Compiler, flag)),
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
            for prefix in ["D", "o"] {
                if flag.starts_with(prefix) {
                    return Some((prefix, Scope::Shared, false));
                }
            }
            for prefix in ["x"] {
                if flag.starts_with(prefix) {
                    return Some((prefix, Scope::Ignore, false));
                }
            }
            for prefix in ["I", "MF"] {
                if flag.starts_with(prefix) {
                    return Some((prefix, Scope::Preprocessor, false));
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
            Arg::param(Scope::Ignore, "x", "c++-header", true),
            Arg::flag(Scope::Shared, "pipe"),
            Arg::flag(Scope::Compiler, "Wall"),
            Arg::flag(Scope::Compiler, "Werror"),
            Arg::flag(Scope::Shared, "funwind-tables"),
            Arg::flag(Scope::Compiler, "Wsequence-point"),
            Arg::flag(Scope::Shared, "mmmx"),
            Arg::flag(Scope::Shared, "msse"),
            Arg::flag(Scope::Shared, "msse2"),
            Arg::flag(Scope::Shared, "fno-math-errno"),
            Arg::flag(Scope::Shared, "fno-rtti"),
            Arg::flag(Scope::Shared, "g3"),
            Arg::flag(Scope::Shared, "gdwarf-3"),
            Arg::flag(Scope::Shared, "O2"),
            Arg::param(Scope::Shared, "D", "_LINUX64", true),
            Arg::param(Scope::Preprocessor, "I", "Engine/Source", true),
            Arg::param(Scope::Preprocessor, "I", "Developer/Public", true),
            Arg::param(Scope::Preprocessor, "I", "Runtime/Core/Private", true),
            Arg::param(Scope::Shared, "D", "IS_PROGRAM=1", true),
            Arg::param(Scope::Shared, "D", "UNICODE", true),
            Arg::param(Scope::Shared, "D", "IS_MONOLITHIC=1", true),
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
            Arg::param(
                Scope::Preprocessor,
                "include-pch",
                "CorePrivatePCH.h.pch",
                true
            ),
            Arg::flag(Scope::Shared, "pipe"),
            Arg::flag(Scope::Compiler, "Wall"),
            Arg::flag(Scope::Compiler, "Werror"),
            Arg::flag(Scope::Shared, "funwind-tables"),
            Arg::flag(Scope::Compiler, "Wsequence-point"),
            Arg::flag(Scope::Shared, "mmmx"),
            Arg::flag(Scope::Shared, "msse"),
            Arg::flag(Scope::Shared, "msse2"),
            Arg::flag(Scope::Shared, "fno-math-errno"),
            Arg::flag(Scope::Shared, "fno-rtti"),
            Arg::flag(Scope::Shared, "g3"),
            Arg::flag(Scope::Shared, "gdwarf-3"),
            Arg::flag(Scope::Shared, "O2"),
            Arg::param(Scope::Shared, "D", "IS_PROGRAM=1", true),
            Arg::param(Scope::Shared, "D", "UNICODE", true),
            Arg::param(Scope::Shared, "D", "IS_MONOLITHIC=1", true),
            Arg::param(Scope::Ignore, "x", "c++", true),
            Arg::flag(Scope::Shared, "std=c++11"),
            Arg::param(Scope::Preprocessor, "include", "CorePrivatePCH.h", true),
            Arg::output(OutputKind::Object, "o", "Module.Core.cpp.o"),
            Arg::input(InputKind::Source, "", "Module.Core.cpp")
        ]
    )
}
