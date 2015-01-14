#![allow(unstable)]
extern crate octobuild;

use octobuild::wincmd;

use std::os;
use std::slice::{Iter};

// Scope of command line argument.
#[derive(Show)]
#[derive(PartialEq)]
enum Scope {
// Preprocessing argument
Preprocessor,
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
Output{kind:OutputKind, flag: String, file: String},
Unknown {arg:String}
}

fn main() {
	println!("Arguments (raw):");
	for arg in os::args()[1..].iter() {
		println!("  {}", arg);
	}
	println!("Arguments (parsed):");
	for arg in parse_arguments(&os::args()[1..]).iter(){
		println!("  {:?}", arg);
	}
}

fn parse_arguments(args: &[String]) -> Vec<Arg> {
	let mut result: Vec<Arg> = Vec::new();
	let mut iter = args.iter();
	loop {
		match parse_argument(&mut iter) {
				Some(arg) => {
					result.push(arg);
			}
				None => {
				break;
			}
			}
	}
	result
}

fn parse_argument(iter: &mut  Iter<String>) -> Option<Arg> {
	match iter.next() {
			Some(arg) => {
				Some(
					if is_param(arg) {
						match arg[1..].as_slice() {
								"c" => {
								Arg::Flag{scope: Scope::Ignore, flag:"c".to_string()}
							}
								"D" => {
								match iter.next() {
										Some(value) if !is_param(value) => {
										Arg::Param{scope: Scope::Preprocessor, flag:"D".to_string(), value:value.to_string()}
									}
										_ => {
										Arg::Unknown{arg:arg.to_string()}
									}
									}
							}
								s if s.starts_with("D") => {
								Arg::Param{scope: Scope::Preprocessor, flag:"D".to_string(), value:s[1..].to_string()}
							}
								s if s.starts_with("Fo") => {
								Arg::Output{kind:OutputKind::Object, flag:"Fo".to_string(), file:s[2..].to_string()}
							}
								s if s.starts_with("Fp") => {
								Arg::Input{kind:InputKind::Precompiled, flag:"Fp".to_string(), file:s[2..].to_string()}
							}
								s if s.starts_with("Yu") => {
								Arg::Input{kind:InputKind::Marker, flag:"Yu".to_string(), file:s[2..].to_string()}
							}
								_ => {
								Arg::Unknown{arg:arg.to_string()}
							}
							}
					} else {
						Arg::Input{kind:InputKind::Source, flag:String::new(), file:arg.to_string()}
					})
		}
			None => {
			None
		}
		}
}

fn is_param(arg: &String) -> bool {
			arg.starts_with("/") || arg.starts_with("-")
}

#[test]
fn test_parse_argument() {
	assert_eq!(
	parse_arguments(&wincmd::parse("/c /Yusample.h /Fpsample.h.pch /Fosample.cpp.o /DTEST /D TEST2 sample.cpp")[]),
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