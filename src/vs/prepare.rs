use std::slice::Iter;
use std::old_io::Command;
use std::ascii::AsciiExt;

use super::super::compiler::{Arg, CompilationTask, Scope, InputKind, OutputKind};
use super::super::utils::filter;

pub fn create_task(command: &Command, args: &[String]) -> Result<CompilationTask, String> {
	match parse_arguments(args) {
		Ok(parsed_args) => {
			// Source file name.
			let input_source = match filter(&parsed_args, |arg:&Arg|->Option<Path> {
				match *arg {
					Arg::Input{ref kind, ref file, ..} if *kind == InputKind::Source => {Some(Path::new(file))}
					_ => {None}
				}
			}).as_slice() {
				[] => {return Err(format!("Can't find source file path."));}
				[ref v] => v.clone(),
				v => {return Err(format!("Found too many source files: {:?}", v));}
			};
			// Precompiled header file name.
			let precompiled_file = match filter(&parsed_args, |arg:&Arg|->Option<Path>{
				match *arg {
					Arg::Input{ref kind, ref file, ..} if *kind == InputKind::Precompiled => {Some(Path::new(file))}
					_ => {None}
				}
			}).as_slice() {
				[] => None,
				[ref v] => Some(v.clone()),
				v => {return Err(format!("Found too many precompiled header files: {:?}", v));}
			};
			// Precompiled header file name.
			let marker_precompiled;
			let input_precompiled;
			let output_precompiled;
			match filter(&parsed_args, |arg:&Arg|->Option<(bool, String)>{
				match *arg {
					Arg::Input{ref kind, ref file, ..} if *kind == InputKind::Marker => Some((true, file.clone())),
					Arg::Output{ref kind, ref file, ..} if *kind == OutputKind::Marker => Some((false, file.clone())),
					_ => None
				}
			}).as_slice() {
				[] => {
					marker_precompiled=None;
					input_precompiled=None;
					output_precompiled=None;
				}
				[(ref input, ref path)] => {
					marker_precompiled=if path.len() > 0 {Some(path.clone())} else {None};
					let precompiled_path = match precompiled_file {
						Some(v) => v,
						None => Path::new(path).with_extension(".pch")
					};
					if *input {
						output_precompiled=None;
						input_precompiled=Some(precompiled_path);
					} else {
						input_precompiled=None;
						output_precompiled=Some(precompiled_path);
					}
				}
				v => {
					return Err(format!("Found too many precompiled header markers: {:?}", v));
				}
			};
			// Output object file name.
			let output_object = match filter(&parsed_args, |arg:&Arg|->Option<Path> {
				match *arg {
					Arg::Output{ref kind, ref file, ..} if *kind == OutputKind::Object => Some(Path::new(file)),
					_ => None
				}
			}).as_slice() {
				[] => input_source.with_extension("obj"),
				[ref v] => v.clone(),
				v => {
					return Err(format!("Found too many output object files: {:?}", v));
				}
			};
			// Language
			let language: String;
			match filter(&parsed_args, |arg:&Arg|->Option<String>{
				match arg {
					&Arg::Param{ref flag, ref value, ..} if *flag == "T" => Some(value.clone()),
					_ => None
				}
			}).as_slice() {
				[]  => {
				match input_source.extension_str() {
						Some(e) if e.eq_ignore_ascii_case("cpp") => {language = "P".to_string();}
						Some(e) if e.eq_ignore_ascii_case("c") => {language = "C".to_string();}
						_ => {
						return Err(format!("Can't detect file language by extension: {:?}", input_source));
					}
				}
			}
			[ref v] => {
				match v.as_slice() {
						"P" | "C" => {language = v.clone();}
						_ => { return Err(format!("Unknown source language type: {}", v));}
					}
				}
				v => {
					return Err(format!("Found too many output object files: {:?}", v));
				}
			};

			Ok(CompilationTask{
				command: command.clone(),
				args: parsed_args,
				language: language,
				input_source: input_source,
				input_precompiled: input_precompiled,
				output_object: output_object,
				output_precompiled: output_precompiled,
				marker_precompiled: marker_precompiled,
			})
		}
			Err(e) => {Err(e)}
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
				let flag = arg[1..].as_slice();
				match is_spaceable_param(flag) {
					Some((prefix, scope)) => {
						if flag == prefix {
							match iter.next() {
								Some(value) if !has_param_prefix(value) => Ok(Arg::Param{scope: scope, flag:prefix.to_string(), value:value.to_string()}),
								_ => Err(arg.to_string())
							}
						} else {
							Ok(Arg::Param{scope: scope, flag:prefix.to_string(), value:flag[prefix.len()..].to_string()})
						}
					}
					None => {
						match flag {
							"c" => Ok(Arg::Flag{scope: Scope::Ignore, flag:flag.to_string()}),
							"bigobj" | "nologo" => Ok(Arg::Flag{scope: Scope::Compiler, flag:flag.to_string()}),
							s if s.starts_with("T") => Ok(Arg::Flag{scope: Scope::Ignore, flag:flag.to_string()}),
							s if s.starts_with("O") => Ok(Arg::Flag{scope: Scope::Shared, flag:flag.to_string()}),
							s if s.starts_with("G") => Ok(Arg::Flag{scope: Scope::Shared, flag:flag.to_string()}),
							s if s.starts_with("RTC") => Ok(Arg::Flag{scope: Scope::Shared, flag:flag.to_string()}),
							s if s.starts_with("Z") => Ok(Arg::Flag{scope: Scope::Shared, flag:flag.to_string()}),
							s if s.starts_with("MD") => Ok(Arg::Flag{scope: Scope::Shared, flag:flag.to_string()}),
							s if s.starts_with("MT") => Ok(Arg::Flag{scope: Scope::Shared, flag:flag.to_string()}),
							s if s.starts_with("EH") => Ok(Arg::Flag{scope: Scope::Shared, flag:flag.to_string()}),
							s if s.starts_with("fp:") => Ok(Arg::Flag{scope: Scope::Shared, flag:flag.to_string()}),
							s if s.starts_with("errorReport:") => Ok(Arg::Flag{scope: Scope::Shared, flag:flag.to_string()}),
							s if s.starts_with("Fo") => Ok(Arg::Output{kind:OutputKind::Object, flag:"Fo".to_string(), file:s[2..].to_string()}),
							s if s.starts_with("Fp") => Ok(Arg::Input{kind:InputKind::Precompiled, flag:"Fp".to_string(), file:s[2..].to_string()}),
							s if s.starts_with("Yc") => Ok(Arg::Output{kind:OutputKind::Marker, flag:"Yc".to_string(), file:s[2..].to_string()}),
							s if s.starts_with("Yu") => Ok(Arg::Input{kind:InputKind::Marker, flag:"Yu".to_string(), file:s[2..].to_string()}),
							s if s.starts_with("FI") => Ok(Arg::Param{scope: Scope::Preprocessor, flag:"FI".to_string(), value:s[2..].to_string()}),
							_ => Err(arg.to_string())
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
	for prefix in ["I", "D"].iter() {
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

fn has_param_prefix(arg: &String) -> bool {
	arg.starts_with("/") || arg.starts_with("-")
}


#[test]
fn test_parse_argument() {
	use super::super::wincmd;

	assert_eq!(
		parse_arguments(&wincmd::parse("/c /Yusample.h /Fpsample.h.pch /Fosample.cpp.o /DTEST /D TEST2 sample.cpp")[]).unwrap(),
		[
			Arg::Flag { scope: Scope::Ignore, flag: "c".to_string()},
			Arg::Input { kind: InputKind::Marker, flag: "Yu".to_string(), file: "sample.h".to_string()},
			Arg::Input { kind: InputKind::Precompiled, flag: "Fp".to_string(), file: "sample.h.pch".to_string()},
			Arg::Output { kind: OutputKind::Object, flag: "Fo".to_string(), file: "sample.cpp.o".to_string()},
			Arg::Param { scope: Scope::Preprocessor, flag: "D".to_string(), value: "TEST".to_string()},
			Arg::Param { scope: Scope::Preprocessor, flag: "D".to_string(), value: "TEST2".to_string()},
			Arg::Input { kind: InputKind::Source, flag: "".to_string(), file: "sample.cpp".to_string()}
		]
	)
}
