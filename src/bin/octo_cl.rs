#![allow(unstable)]
extern crate octobuild;
extern crate log;

use octobuild::wincmd;

use std::os;
use std::slice::{Iter};

use std::io::{Command, File, BufferedReader};

// Scope of command line argument.
#[derive(Show)]
#[derive(PartialEq)]
enum Scope {
// Preprocessing argument
Preprocessor,
// Compiler argument
Compiler,
// Preprocessor & compiler argument
Shared,
// Unknown argument - local build only
Ignore,
}

#[derive(Show)]
#[derive(PartialEq)]
enum InputKind {
Source,
Marker,
Precompiled,
}

#[derive(Show)]
#[derive(PartialEq)]
enum OutputKind {
Object,
}

#[derive(Show)]
#[derive(PartialEq)]
enum Arg {
Flag{scope:Scope, flag: String},
Param{scope:Scope, flag: String, value: String},
Input{kind:InputKind, flag: String, file: String},
Output{kind:OutputKind, flag: String, file: String}
}

struct CompilationTask {
// Parsed arguments.
args: Vec<Arg>,
// Source language.
language: String,
// Input source file name.
source: String,
// Input precompiled header file name.
precompiled: String,
// Output object file name.
output: String,
}

fn main() {
	println!("Arguments (raw):");
	for arg in os::args()[1..].iter() {
		println!("  {}", arg);
	}
	println!("Arguments (parsed):");
	match parse_arguments(&os::args()[1..]) {
			Ok(parsed_args) => {
			for arg in parsed_args.iter(){
				println!("  {:?}", arg);
			}
		}
			Err(e) => {println!("{}", e);}
		}

	match Command::new("cl.exe")
	.args(os::args()[1..].as_slice())
	.output(){
			Ok(output) => {
			println!("stdout: {}", String::from_utf8_lossy(output.output.as_slice()));
			println!("stderr: {}", String::from_utf8_lossy(output.error.as_slice()));
		}
			Err(e) => {
			panic!("{}", e);
		}
		}
}

fn parse_arguments(args: &[String]) -> Result<Vec<Arg>, String> {
	let mut result: Vec<Arg> = Vec::new();
	let mut errors: Vec<String> = Vec::new();
	let mut iter = args.iter();
	loop {
		match parse_argument(&mut iter) {
				Some(parse_result) => {
				match (parse_result) {
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
			Some(arg) => {
				Some(
					if has_param_prefix(arg) {
						let flag = arg[1..].as_slice();
						match is_spaceable_param(flag) {
								Some((prefix, scope)) => {
								if flag == prefix {
									match iter.next() {
											Some(value) if !has_param_prefix(value) => {
												Ok(Arg::Param{scope: scope, flag:prefix.to_string(), value:value.to_string()})
										}
											_ => {
												Err(arg.to_string())
										}
										}
								} else {
										Ok(Arg::Param{scope: scope, flag:prefix.to_string(), value:flag[prefix.len()..].to_string()})
								}
							}
								None => {
								match flag {
										"c" | "nologo" => {
											Ok(Arg::Flag{scope: Scope::Ignore, flag:flag.to_string()})
									}
										"bigobj" => {
											Ok(Arg::Flag{scope: Scope::Compiler, flag:flag.to_string()})
									}
										s if s.starts_with("T") => {
											Ok(Arg::Flag{scope: Scope::Ignore, flag:flag.to_string()})
									}
										s if s.starts_with("O") => {
											Ok(Arg::Flag{scope: Scope::Compiler, flag:flag.to_string()})
									}
										s if s.starts_with("G") => {
											Ok(Arg::Flag{scope: Scope::Compiler, flag:flag.to_string()})
									}
										s if s.starts_with("RTC") => {
											Ok(Arg::Flag{scope: Scope::Compiler, flag:flag.to_string()})
									}
										s if s.starts_with("Z") => {
											Ok(Arg::Flag{scope: Scope::Compiler, flag:flag.to_string()})
									}
										s if s.starts_with("MD") => {
											Ok(Arg::Flag{scope: Scope::Compiler, flag:flag.to_string()})
									}
										s if s.starts_with("MT") => {
											Ok(Arg::Flag{scope: Scope::Compiler, flag:flag.to_string()})
									}
										s if s.starts_with("EH") => {
											Ok(Arg::Flag{scope: Scope::Compiler, flag:flag.to_string()})
									}
										s if s.starts_with("fp:") => {
											Ok(Arg::Flag{scope: Scope::Compiler, flag:flag.to_string()})
									}
										s if s.starts_with("errorReport:") => {
											Ok(Arg::Flag{scope: Scope::Shared, flag:flag.to_string()})
									}
										s if s.starts_with("Fo") => {
											Ok(Arg::Output{kind:OutputKind::Object, flag:"Fo".to_string(), file:s[2..].to_string()})
									}
										s if s.starts_with("Fp") => {
											Ok(Arg::Input{kind:InputKind::Precompiled, flag:"Fp".to_string(), file:s[2..].to_string()})
									}
										s if s.starts_with("Yu") => {
											Ok(Arg::Input{kind:InputKind::Marker, flag:"Yu".to_string(), file:s[2..].to_string()})
									}
										_ => {
											Err(arg.to_string())
									}
									}
							}
							}
					} else {
							Ok(Arg::Input{kind:InputKind::Source, flag:String::new(), file:arg.to_string()})
					})
		}
			None => {
			None
		}
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

#[test]
fn test_precompiled_header()  {
		wincmd::parse("/c /Ycsample.h /Fpsample.h.pch /Foprecompiled.cpp.o precompiled.cpp");
}

#[test]
fn test_compile_no_header()   {
		wincmd::parse("/c /Fosample.cpp.o sample.cpp");
}

#[test]
fn test_compile_with_header() {
		wincmd::parse("/c /Yusample.h /Fpsample.h.pch /Fosample.cpp.o sample.cpp");
}