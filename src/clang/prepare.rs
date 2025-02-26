use crate::compiler::{
    Arg, CommandInfo, CompilationArgs, CompilationTask, InputKind, OutputKind, PCHUsage, ParamForm,
    Scope,
};
use crate::utils::{expand_response_files, find_param, ParamValue};
use std::path::PathBuf;
use std::sync::Arc;
use std::vec::IntoIter;

pub fn create_tasks(
    command: CommandInfo,
    args: Vec<String>,
    run_second_cpp: bool,
) -> crate::Result<Vec<CompilationTask>> {
    let expanded_args = expand_response_files(&command.current_dir, args)?;

    if expanded_args.iter().any(|v| v == "--analyze") {
        // Support only compilation steps
        return Ok(Vec::new());
    }

    if !expanded_args.iter().any(|v| matches!(v as &str, "-c")) {
        // Support only compilation steps
        return Ok(Vec::new());
    }

    let parsed_args = parse_arguments(expanded_args)?;
    // Source file name.
    let input_sources: Vec<PathBuf> = parsed_args
        .iter()
        .filter_map(|arg| match arg {
            Arg::Input { kind, file, .. } if *kind == InputKind::Source => Some(file.clone()),
            _ => None,
        })
        .collect();
    if input_sources.is_empty() {
        return Err(crate::Error::from("Can't find source file path."));
    }
    /*
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
     */

    // Output object file name.
    let output_object = match find_param(
        &parsed_args,
        |arg: &Arg| -> Option<crate::Result<PathBuf>> {
            match arg {
                Arg::Output { kind, file, .. } if *kind == OutputKind::Object => {
                    Some(command.absolutize(file))
                }
                _ => None,
            }
        },
    ) {
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
    }
    .map_or(Ok(None), |v| v.map(Some))?;

    let deps_file = parsed_args
        .iter()
        .find_map(|arg| match arg {
            Arg::Output { kind, file, .. } if *kind == OutputKind::Deps => {
                Some(command.absolutize(file))
            }
            _ => None,
        })
        .map_or(Ok(None), |v| v.map(Some))?;

    // Language
    let language: Option<String> = match find_param(&parsed_args, |arg: &Arg| -> Option<String> {
        match arg {
            Arg::Param {
                name: flag, value, ..
            } if *flag == "x" => Some(value.clone()),
            _ => None,
        }
    }) {
        ParamValue::None => None,
        ParamValue::Single(v) => {
            match &v[..] {
                "c" | "c++" | "objective-c++" => Some(v.to_string()),
                "c-header" | "c++-header" | "objective-c++-header" => {
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
        // No PCH support for clang for now
        pch_usage: PCHUsage::None,
        deps_file,
        run_second_cpp,
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

fn parse_arguments(args: Vec<String>) -> Result<Vec<Arg>, String> {
    let mut result: Vec<Arg> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    let mut iter = args.into_iter();
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

struct CompilerArgument {
    scope: Scope,
    name: &'static str,
    value_type: &'static [ArgValueType],
}

#[derive(Debug, Eq, PartialEq)]
enum ArgValueType {
    None,
    Separate,
    Combined,
    StartsWith,
}

const NORMAL: &[ArgValueType] = &[ArgValueType::Separate, ArgValueType::Combined];
const NONE: &[ArgValueType] = &[ArgValueType::None];
const COMBINED: &[ArgValueType] = &[ArgValueType::Combined];
const PSYCHEDELIC: &[ArgValueType] = &[ArgValueType::Separate, ArgValueType::StartsWith];
const STARTS_WITH: &[ArgValueType] = &[ArgValueType::StartsWith];
const OPTIONAL_STARTS_WITH: &[ArgValueType] = &[ArgValueType::None, ArgValueType::StartsWith];
const SEPARATE: &[ArgValueType] = &[ArgValueType::Separate];

static DASH_DASH_PARAMS: &[CompilerArgument] = &[
    CompilerArgument {
        scope: Scope::Shared,
        name: "driver-mode",
        value_type: COMBINED,
    },
    CompilerArgument {
        scope: Scope::Shared,
        name: "gcc-toolchain",
        value_type: COMBINED,
    },
    CompilerArgument {
        scope: Scope::Shared,
        name: "sysroot",
        value_type: NORMAL,
    },
    CompilerArgument {
        scope: Scope::Shared,
        name: "target",
        value_type: COMBINED,
    },
];

static DASH_PARAMS: &[CompilerArgument] = &[
    // Shared
    CompilerArgument {
        scope: Scope::Shared,
        name: "arch",
        value_type: NORMAL,
    },
    CompilerArgument {
        scope: Scope::Shared,
        name: "D",
        value_type: PSYCHEDELIC,
    },
    CompilerArgument {
        scope: Scope::Shared,
        name: "f",
        value_type: STARTS_WITH,
    },
    CompilerArgument {
        scope: Scope::Shared,
        name: "g",
        value_type: OPTIONAL_STARTS_WITH,
    },
    CompilerArgument {
        scope: Scope::Shared,
        name: "m",
        value_type: STARTS_WITH,
    },
    CompilerArgument {
        scope: Scope::Shared,
        name: "no-canonical-prefixes",
        value_type: NONE,
    },
    CompilerArgument {
        scope: Scope::Shared,
        name: "nostdinc++",
        value_type: NONE,
    },
    CompilerArgument {
        scope: Scope::Shared,
        name: "O",
        value_type: STARTS_WITH,
    },
    CompilerArgument {
        scope: Scope::Shared,
        name: "o",
        value_type: SEPARATE,
    },
    CompilerArgument {
        scope: Scope::Shared,
        name: "pipe",
        value_type: NONE,
    },
    CompilerArgument {
        scope: Scope::Shared,
        name: "sce-stdlib",
        value_type: COMBINED,
    },
    CompilerArgument {
        scope: Scope::Shared,
        name: "std",
        value_type: COMBINED,
    },
    CompilerArgument {
        scope: Scope::Shared,
        name: "stdlib",
        value_type: COMBINED,
    },
    CompilerArgument {
        scope: Scope::Shared,
        name: "target",
        value_type: SEPARATE,
    },
    // Preprocessor
    CompilerArgument {
        scope: Scope::Preprocessor,
        name: "F",
        value_type: STARTS_WITH,
    },
    CompilerArgument {
        scope: Scope::Preprocessor,
        name: "I",
        value_type: PSYCHEDELIC,
    },
    CompilerArgument {
        scope: Scope::Preprocessor,
        name: "include",
        value_type: NORMAL,
    },
    CompilerArgument {
        scope: Scope::Preprocessor,
        name: "include-pch",
        value_type: NORMAL,
    },
    CompilerArgument {
        scope: Scope::Preprocessor,
        name: "isysroot",
        value_type: NORMAL,
    },
    CompilerArgument {
        scope: Scope::Preprocessor,
        name: "isystem",
        value_type: PSYCHEDELIC,
    },
    CompilerArgument {
        scope: Scope::Preprocessor,
        name: "MD",
        value_type: NONE,
    },
    CompilerArgument {
        scope: Scope::Preprocessor,
        name: "MF",
        value_type: PSYCHEDELIC,
    },
    // Compiler
    CompilerArgument {
        scope: Scope::Compiler,
        name: "W",
        value_type: STARTS_WITH,
    },
    // Ignore
    CompilerArgument {
        scope: Scope::Ignore,
        name: "c",
        value_type: NONE,
    },
    CompilerArgument {
        scope: Scope::Ignore,
        name: "x",
        value_type: NORMAL,
    },
];

fn handle_argument(
    prefix: &'static str,
    key: &str,
    params: &[CompilerArgument],
    iter: &mut IntoIter<String>,
) -> Option<Arg> {
    for param in params {
        for value_type in param.value_type {
            match value_type {
                ArgValueType::None => {
                    if key == param.name {
                        return Some(Arg::flag(param.scope, prefix, key));
                    }
                }
                ArgValueType::Separate => {
                    if key == param.name {
                        return iter
                            .next()
                            .map(|v| Arg::param(param.scope, prefix, param.name, v));
                    }
                }
                ArgValueType::Combined => {
                    if key.starts_with(format!("{}=", param.name).as_str()) {
                        return Some(Arg::param_ext(
                            param.scope,
                            prefix,
                            param.name,
                            &key[param.name.len() + 1..],
                            if param.value_type == NORMAL {
                                ParamForm::Separate
                            } else {
                                ParamForm::Combined
                            },
                        ));
                    }
                }
                ArgValueType::StartsWith => {
                    if let Some(v) = key.strip_prefix(param.name) {
                        if !v.is_empty() {
                            return Some(Arg::param_ext(
                                param.scope,
                                prefix,
                                param.name,
                                v,
                                if param.value_type == PSYCHEDELIC {
                                    ParamForm::Separate
                                } else {
                                    ParamForm::Smushed
                                },
                            ));
                        }
                    }
                }
            }
        }
    }

    None
}

fn parse_argument(iter: &mut IntoIter<String>) -> Option<Result<Arg, String>> {
    iter.next().map(|arg| {
        if let Some(key) = arg.strip_prefix("--") {
            match handle_argument("--", key, DASH_DASH_PARAMS, iter) {
                Some(v) => Ok(v),
                None => Err(arg),
            }
        } else if let Some(key) = arg.strip_prefix('-') {
            match handle_argument("-", key, DASH_PARAMS, iter) {
                Some(v) => match &v {
                    Arg::Param {
                        scope: _,
                        prefix: _,
                        name: flag,
                        value,
                        form: _,
                    } => {
                        if flag == "o" {
                            // Minor hack
                            Ok(Arg::Output {
                                kind: OutputKind::Object,
                                name: flag.into(),
                                file: PathBuf::from(value),
                            })
                        } else if flag == "MF" {
                            Ok(Arg::Output {
                                kind: OutputKind::Deps,
                                name: flag.into(),
                                file: PathBuf::from(value),
                            })
                        } else {
                            Ok(v)
                        }
                    }
                    _ => Ok(v),
                },
                None => Err(arg),
            }
        } else {
            Ok(Arg::Input {
                kind: InputKind::Source,
                file: PathBuf::from(arg),
            })
        }
    })
}

#[test]
fn test_parse_argument_precompile() {
    let args: Vec<String> =
        "-x c++-header -pipe -Wall -Werror -funwind-tables -Wsequence-point -mmmx -msse -msse2 \
         -fno-math-errno -fno-rtti -g -g3 -gdwarf-3 -O2 -D_LINUX64 -IEngine/Source \
         -IDeveloper/Public -I Runtime/Core/Private -D IS_PROGRAM=1 -D UNICODE \
         -MD -nostdinc++ --gcc-toolchain=/bla/bla -no-canonical-prefixes \
         -MFpath/to/file \
         -target bla \
         --target=android \
         -isystemPATH \
         -stdlib=libc++ \
         -DIS_MONOLITHIC=1 -std=c++11 -o CorePrivatePCH.h.pch CorePrivatePCH.h"
            .split(' ')
            .map(|x| x.to_string())
            .collect();
    assert_eq!(
        parse_arguments(args).unwrap(),
        [
            Arg::param(Scope::Ignore, "-", "x", "c++-header"),
            Arg::flag(Scope::Shared, "-", "pipe"),
            Arg::param_ext(Scope::Compiler, "-", "W", "all", ParamForm::Smushed),
            Arg::param_ext(Scope::Compiler, "-", "W", "error", ParamForm::Smushed),
            Arg::param_ext(Scope::Shared, "-", "f", "unwind-tables", ParamForm::Smushed),
            Arg::param_ext(
                Scope::Compiler,
                "-",
                "W",
                "sequence-point",
                ParamForm::Smushed
            ),
            Arg::param_ext(Scope::Shared, "-", "m", "mmx", ParamForm::Smushed),
            Arg::param_ext(Scope::Shared, "-", "m", "sse", ParamForm::Smushed),
            Arg::param_ext(Scope::Shared, "-", "m", "sse2", ParamForm::Smushed),
            Arg::param_ext(Scope::Shared, "-", "f", "no-math-errno", ParamForm::Smushed),
            Arg::param_ext(Scope::Shared, "-", "f", "no-rtti", ParamForm::Smushed),
            Arg::flag(Scope::Shared, "-", "g"),
            Arg::param_ext(Scope::Shared, "-", "g", "3", ParamForm::Smushed),
            Arg::param_ext(Scope::Shared, "-", "g", "dwarf-3", ParamForm::Smushed),
            Arg::param_ext(Scope::Shared, "-", "O", "2", ParamForm::Smushed),
            Arg::param(Scope::Shared, "-", "D", "_LINUX64"),
            Arg::param(Scope::Preprocessor, "-", "I", "Engine/Source"),
            Arg::param(Scope::Preprocessor, "-", "I", "Developer/Public"),
            Arg::param(Scope::Preprocessor, "-", "I", "Runtime/Core/Private"),
            Arg::param(Scope::Shared, "-", "D", "IS_PROGRAM=1"),
            Arg::param(Scope::Shared, "-", "D", "UNICODE"),
            Arg::flag(Scope::Preprocessor, "-", "MD"),
            Arg::flag(Scope::Shared, "-", "nostdinc++"),
            Arg::param_ext(
                Scope::Shared,
                "--",
                "gcc-toolchain",
                "/bla/bla",
                ParamForm::Combined
            ),
            Arg::flag(Scope::Shared, "-", "no-canonical-prefixes"),
            Arg::Output {
                kind: OutputKind::Deps,
                name: "MF".into(),
                file: "path/to/file".into()
            },
            Arg::param(Scope::Shared, "-", "target", "bla"),
            Arg::param_ext(
                Scope::Shared,
                "--",
                "target",
                "android",
                ParamForm::Combined
            ),
            Arg::param(Scope::Preprocessor, "-", "isystem", "PATH"),
            Arg::param_ext(Scope::Shared, "-", "stdlib", "libc++", ParamForm::Combined),
            Arg::param(Scope::Shared, "-", "D", "IS_MONOLITHIC=1"),
            Arg::param_ext(Scope::Shared, "-", "std", "c++11", ParamForm::Combined),
            Arg::Output {
                kind: OutputKind::Object,
                name: "o".into(),
                file: "CorePrivatePCH.h.pch".into(),
            },
            Arg::Input {
                kind: InputKind::Source,
                file: "CorePrivatePCH.h".into(),
            }
        ]
    )
}

#[test]
fn test_parse_argument_compile() {
    let args: Vec<String> =
        "-c -include-pch CorePrivatePCH.h.pch -pipe -Wall -Werror -funwind-tables \
         -Wsequence-point -mmmx -msse -msse2 -fno-math-errno -fno-rtti -g3 -gdwarf-3 -O2 -D \
         IS_PROGRAM=1 -D UNICODE -DIS_MONOLITHIC=1 -x c++ -std=c++11 -include CorePrivatePCH.h \
         --driver-mode=g++ -sce-stdlib=v1 \
         -o Module.Core.cpp.o Module.Core.cpp"
            .split(' ')
            .map(|x| x.to_string())
            .collect();
    assert_eq!(
        parse_arguments(args).unwrap(),
        [
            Arg::flag(Scope::Ignore, "-", "c"),
            Arg::param(
                Scope::Preprocessor,
                "-",
                "include-pch",
                "CorePrivatePCH.h.pch",
            ),
            Arg::flag(Scope::Shared, "-", "pipe"),
            Arg::param_ext(Scope::Compiler, "-", "W", "all", ParamForm::Smushed),
            Arg::param_ext(Scope::Compiler, "-", "W", "error", ParamForm::Smushed),
            Arg::param_ext(Scope::Shared, "-", "f", "unwind-tables", ParamForm::Smushed),
            Arg::param_ext(
                Scope::Compiler,
                "-",
                "W",
                "sequence-point",
                ParamForm::Smushed
            ),
            Arg::param_ext(Scope::Shared, "-", "m", "mmx", ParamForm::Smushed),
            Arg::param_ext(Scope::Shared, "-", "m", "sse", ParamForm::Smushed),
            Arg::param_ext(Scope::Shared, "-", "m", "sse2", ParamForm::Smushed),
            Arg::param_ext(Scope::Shared, "-", "f", "no-math-errno", ParamForm::Smushed),
            Arg::param_ext(Scope::Shared, "-", "f", "no-rtti", ParamForm::Smushed),
            Arg::param_ext(Scope::Shared, "-", "g", "3", ParamForm::Smushed),
            Arg::param_ext(Scope::Shared, "-", "g", "dwarf-3", ParamForm::Smushed),
            Arg::param_ext(Scope::Shared, "-", "O", "2", ParamForm::Smushed),
            Arg::param(Scope::Shared, "-", "D", "IS_PROGRAM=1"),
            Arg::param(Scope::Shared, "-", "D", "UNICODE"),
            Arg::param(Scope::Shared, "-", "D", "IS_MONOLITHIC=1"),
            Arg::param(Scope::Ignore, "-", "x", "c++"),
            Arg::param_ext(Scope::Shared, "-", "std", "c++11", ParamForm::Combined),
            Arg::param(Scope::Preprocessor, "-", "include", "CorePrivatePCH.h"),
            Arg::param_ext(
                Scope::Shared,
                "--",
                "driver-mode",
                "g++",
                ParamForm::Combined
            ),
            Arg::param_ext(Scope::Shared, "-", "sce-stdlib", "v1", ParamForm::Combined),
            Arg::Output {
                kind: OutputKind::Object,
                name: "o".into(),
                file: "Module.Core.cpp.o".into(),
            },
            Arg::Input {
                kind: InputKind::Source,
                file: "Module.Core.cpp".into(),
            },
        ]
    )
}
