use std::iter::FromIterator;
use std::slice::Iter;
use std::ascii::AsciiExt;
use std::path::{Path, PathBuf};

use super::super::compiler::{Arg, CommandInfo, CompilationTask, Scope, InputKind, OutputKind};
use super::super::utils::filter;

enum ParamValue<T> {
	None,
	Single(T),
	Many(Vec<T>),
}

pub fn create_task(command: CommandInfo, args: &[String]) -> Result<CompilationTask, String> {
	match parse_arguments(args) {
		Ok(parsed_args) => {
			// Source file name.
			let input_source = match find_param(&parsed_args, |arg:&Arg|->Option<PathBuf> {
				match arg {
					&Arg::Input{ref kind, ref file, ..} if *kind == InputKind::Source => {Some(Path::new(file).to_path_buf())}
					_ => {None}
				}
			}) {
				ParamValue::None => {return Err(format!("Can't find source file path."));}
				ParamValue::Single(v) => v,
				ParamValue::Many(v) => {return Err(format!("Found too many source files: {:?}", v));}
			};
			// Precompiled header file name.
			let input_precompiled = match find_param(&parsed_args, |arg:&Arg|->Option<PathBuf> {
				match arg {
					&Arg::Input{ref kind, ref file, ..} if *kind == InputKind::Precompiled => {Some(Path::new(file).to_path_buf())}
					_ => {None}
				}
			}) {
				ParamValue::None => None,
				ParamValue::Single(v) => Some(v),
				ParamValue::Many(v) => {return Err(format!("Found too many precompiled header files: {:?}", v));}
			};
			// Precompiled header file name.
			let marker_precompiled = parsed_args.iter().filter_map(|arg| match arg {
				&Arg::Param{ref flag, ref value, ..} if *flag == "include" => Some(value.clone()),
				_ => None,
			}).next();
			// Output object file name.
			let output_object = match find_param(&parsed_args, |arg:&Arg|->Option<PathBuf> {
				match arg {
					&Arg::Output{ref kind, ref file, ..} if *kind == OutputKind::Object => Some(Path::new(file).to_path_buf()),
					_ => None
				}
			}) {
				ParamValue::None => input_source.with_extension("o"),
				ParamValue::Single(v) => v,
				ParamValue::Many(v) => {
					return Err(format!("Found too many output object files: {:?}", v));
				}
			};
			// Language
			let language: String;
			match find_param(&parsed_args, |arg:&Arg|->Option<String>{
				match arg {
					&Arg::Param{ref flag, ref value, ..} if *flag == "x" => Some(value.clone()),
					_ => None
				}
			}) {
				ParamValue::None  => {
					match input_source.extension() {
						Some(extension) => {
							match extension.to_str() {
								Some(e) if e.eq_ignore_ascii_case("cpp") => {language = "c++".to_string();}
								Some(e) if e.eq_ignore_ascii_case("c") => {language = "c".to_string();}
								Some(e) if e.eq_ignore_ascii_case("hpp") => {language = "c++-header".to_string();}
								Some(e) if e.eq_ignore_ascii_case("h") => {language = "c-header".to_string();}
								_ => {return Err(format!("Can't detect file language by extension: {:?}", input_source));}
							}
						}
						_ => {return Err(format!("Can't detect file language by extension: {:?}", input_source));}
					}
				}
				ParamValue::Single(v) => {
					match &v[..] {
						"c" | "c++" => {language = v.clone();}
						"c-header" | "c++-header" => {return Err(format!("Precompiled headers must build locally"));}
						_ => { return Err(format!("Unknown source language type: {}", v));}
					}
				}
				ParamValue::Many(v) => {
					return Err(format!("Found too many output object files: {:?}", v));
				}
			};

			Ok(CompilationTask{
				command: command,
				args: parsed_args,
				language: language,
				input_source: input_source,
				input_precompiled: input_precompiled,
				output_object: output_object,
				output_precompiled: None,
				marker_precompiled: marker_precompiled,
			})
		}
			Err(e) => {Err(e)}
		}
}

fn find_param<T, R, F:Fn(&T) -> Option<R>>(args: &Vec<T>, filter:F) -> ParamValue<R> {
	let mut found = Vec::from_iter(args.iter().filter_map(filter));
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
	loop {
		match parse_argument(&mut iter) {
			Some(parse_result) => {
				match parse_result {
					Ok(arg) => {result.push(arg);}
					Err(e) => {errors.push(e);}
				}
			}
			None => {
				break;
			}
		}
	}
	if errors.len() > 0 {
		return Err(format!("Found unknown command line arguments: {:?}", errors))
	}
	Ok(result)
}

fn parse_argument(iter: &mut  Iter<String>) -> Option<Result<Arg, String>> {
	match iter.next() {
		Some(arg) => Some(
			if has_param_prefix(arg) {
				let flag = &arg[1..];
				match is_spaceable_param(flag) {
					Some((prefix, scope)) => {
						let value = match flag == prefix {
							true => match iter.next() {
								Some(v) if !has_param_prefix(v) => v.to_string(),
								_ => {
									return Some(Err(arg.to_string()));
								}
							},
							false => flag[prefix.len()..].to_string(),
						};
						match flag {
							"o" => Ok(Arg::Output{kind:OutputKind::Object, flag: prefix.to_string(), file: value}),
							_ => Ok(Arg::Param{scope: scope, flag: prefix.to_string(), value: value}),
						}
					}
					None => {
						match flag {
							"c" => Ok(Arg::Flag{scope: Scope::Ignore, flag:flag.to_string()}),
							"pipe" => Ok(Arg::Flag{scope: Scope::Shared, flag:flag.to_string()}),
							s if s.starts_with("f") => Ok(Arg::Flag{scope: Scope::Shared, flag:flag.to_string()}),
							s if s.starts_with("g") => Ok(Arg::Flag{scope: Scope::Shared, flag:flag.to_string()}),
							s if s.starts_with("O") => Ok(Arg::Flag{scope: Scope::Shared, flag:flag.to_string()}),
							s if s.starts_with("W") => Ok(Arg::Flag{scope: Scope::Shared, flag:flag.to_string()}),
							s if s.starts_with("m") => Ok(Arg::Flag{scope: Scope::Shared, flag:flag.to_string()}),
							s if s.starts_with("std=") => Ok(Arg::Flag{scope: Scope::Shared, flag:flag.to_string()}),
							_ => Err(arg.to_string()),
						}
					}
				}
			} else {
				Ok(Arg::Input{kind:InputKind::Source, flag:String::new(), file:arg.to_string()})
		}),
		None => None
	}
}

fn is_spaceable_param(flag: &str) -> Option<(&str, Scope)> {
	match flag {
		"include" | "include-pch" => Some((flag, Scope::Preprocessor)),
		_=> {
			for prefix in ["D", "o"].iter() {
				if flag.starts_with(*prefix) {
					return Some((*prefix, Scope::Shared));
				}
			}
			for prefix in ["x"].iter() {
				if flag.starts_with(*prefix) {
					return Some((*prefix, Scope::Ignore));
				}
			}
			for prefix in ["I"].iter() {
				if flag.starts_with(*prefix) {
					return Some((*prefix, Scope::Preprocessor));
				}
			}
			None
		}
	}
}

fn has_param_prefix(arg: &String) -> bool {
	arg.starts_with("-")
}

#[test]
fn test_parse_argument_precompile() {
	let args = Vec::from_iter("-x c++-header -pipe -Wall -Werror -funwind-tables -Wsequence-point -mmmx -msse -msse2 -fno-math-errno -fno-rtti -g3 -gdwarf-3 -O2 -D_LINUX64 -IEngine/Source -IDeveloper/Public -I Runtime/Core/Private -D IS_PROGRAM=1 -D UNICODE -DIS_MONOLITHIC=1 -std=c++11 -o CorePrivatePCH.h.pch CorePrivatePCH.h".split(" ").map(|x| x.to_string()));
	assert_eq!(
		parse_arguments(&args).unwrap(),
		[
			Arg::Param { scope: Scope::Ignore, flag: "x".to_string(), value: "c++-header".to_string()},
			Arg::Flag { scope: Scope::Shared, flag: "pipe".to_string() },
			Arg::Flag { scope: Scope::Shared, flag: "Wall".to_string() },
			Arg::Flag { scope: Scope::Shared, flag: "Werror".to_string() },
			Arg::Flag { scope: Scope::Shared, flag: "funwind-tables".to_string() },
			Arg::Flag { scope: Scope::Shared, flag: "Wsequence-point".to_string() },
			Arg::Flag { scope: Scope::Shared, flag: "mmmx".to_string() },
			Arg::Flag { scope: Scope::Shared, flag: "msse".to_string() },
			Arg::Flag { scope: Scope::Shared, flag: "msse2".to_string() },
			Arg::Flag { scope: Scope::Shared, flag: "fno-math-errno".to_string() },
			Arg::Flag { scope: Scope::Shared, flag: "fno-rtti".to_string() },
			Arg::Flag { scope: Scope::Shared, flag: "g3".to_string() },
			Arg::Flag { scope: Scope::Shared, flag: "gdwarf-3".to_string() },
			Arg::Flag { scope: Scope::Shared, flag: "O2".to_string() },
			Arg::Param { scope: Scope::Shared, flag: "D".to_string(), value: "_LINUX64".to_string() },
			Arg::Param { scope: Scope::Preprocessor, flag: "I".to_string(), value: "Engine/Source".to_string() },
			Arg::Param { scope: Scope::Preprocessor, flag: "I".to_string(), value: "Developer/Public".to_string() },
			Arg::Param { scope: Scope::Preprocessor, flag: "I".to_string(), value: "Runtime/Core/Private".to_string() },
			Arg::Param { scope: Scope::Shared, flag: "D".to_string(), value: "IS_PROGRAM=1".to_string() },
			Arg::Param { scope: Scope::Shared, flag: "D".to_string(), value: "UNICODE".to_string() },
			Arg::Param { scope: Scope::Shared, flag: "D".to_string(), value: "IS_MONOLITHIC=1".to_string() },
			Arg::Flag { scope: Scope::Shared, flag: "std=c++11".to_string() },
			Arg::Output { kind: OutputKind::Object, flag: "o".to_string(), file: "CorePrivatePCH.h.pch".to_string() },
			Arg::Input { kind: InputKind::Source, flag: "".to_string(), file: "CorePrivatePCH.h".to_string() },
		]
	)
}

#[test]
fn test_parse_argument_compile() {
	let args = Vec::from_iter("-c -include-pch CorePrivatePCH.h.pch -pipe -Wall -Werror -funwind-tables -Wsequence-point -mmmx -msse -msse2 -fno-math-errno -fno-rtti -g3 -gdwarf-3 -O2 -D IS_PROGRAM=1 -D UNICODE -DIS_MONOLITHIC=1 -x c++ -std=c++11 -include CorePrivatePCH.h -o Module.Core.cpp.o Module.Core.cpp".split(" ").map(|x| x.to_string()));
	assert_eq!(
		parse_arguments(&args).unwrap(),
		[
			Arg::Flag { scope: Scope::Ignore, flag: "c".to_string() },
			Arg::Param { scope: Scope::Preprocessor, flag: "include-pch".to_string(), value: "CorePrivatePCH.h.pch".to_string() },
			Arg::Flag { scope: Scope::Shared, flag: "pipe".to_string() },
			Arg::Flag { scope: Scope::Shared, flag: "Wall".to_string() },
			Arg::Flag { scope: Scope::Shared, flag: "Werror".to_string() },
			Arg::Flag { scope: Scope::Shared, flag: "funwind-tables".to_string() },
			Arg::Flag { scope: Scope::Shared, flag: "Wsequence-point".to_string() },
			Arg::Flag { scope: Scope::Shared, flag: "mmmx".to_string() },
			Arg::Flag { scope: Scope::Shared, flag: "msse".to_string() },
			Arg::Flag { scope: Scope::Shared, flag: "msse2".to_string() },
			Arg::Flag { scope: Scope::Shared, flag: "fno-math-errno".to_string() },
			Arg::Flag { scope: Scope::Shared, flag: "fno-rtti".to_string() },
			Arg::Flag { scope: Scope::Shared, flag: "g3".to_string() },
			Arg::Flag { scope: Scope::Shared, flag: "gdwarf-3".to_string() },
			Arg::Flag { scope: Scope::Shared, flag: "O2".to_string() },
			Arg::Param { scope: Scope::Shared, flag: "D".to_string(), value: "IS_PROGRAM=1".to_string() },
			Arg::Param { scope: Scope::Shared, flag: "D".to_string(), value: "UNICODE".to_string() },
			Arg::Param { scope: Scope::Shared, flag: "D".to_string(), value: "IS_MONOLITHIC=1".to_string() },
			Arg::Param { scope: Scope::Ignore, flag: "x".to_string(), value: "c++".to_string() },
			Arg::Flag { scope: Scope::Shared, flag: "std=c++11".to_string() },
			Arg::Param { scope: Scope::Preprocessor, flag: "include".to_string(), value: "CorePrivatePCH.h".to_string() },
			Arg::Output { kind: OutputKind::Object, flag: "o".to_string(), file: "Module.Core.cpp.o".to_string() },
			Arg::Input { kind: InputKind::Source, flag: "".to_string(), file: "Module.Core.cpp".to_string() },
		]
	)
}
