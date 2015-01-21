#![allow(unstable)]
extern crate octobuild;
extern crate log;

use octobuild::wincmd;
use std::ascii::AsciiExt;
use std::str::StrExt;

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
Precompiled,
}

#[derive(Show)]
#[derive(PartialEq)]
enum Arg {
Flag{scope:Scope, flag: String},
Param{scope:Scope, flag: String, value: String},
Input{kind:InputKind, flag: String, file: String},
Output{kind:OutputKind, flag: String, file: String}
}

#[derive(Show)]
struct CompilationTask {
// Parsed arguments.
args: Vec<Arg>,
// Source language.
language: String,
// Input source file name.
inputSource: Path,
// Input precompiled header file name.
inputPrecompiled: Option<Path>,
// Output object file name.
outputObject: Path,
// Output precompiled header file name.
outputPrecompiled: Option<Path>,
// Marker for precompiled header.
markerPrecompiled: Option<String>,
}

fn main() {
	let result = parse_compilation_task(&os::args()[1..]);
	println!("Parsed task: {:?}", result);
	match result {
			Ok(task) => {
				preprocess(&task);
				compile(&task);
		}
			_ => {}
		}

	/*match Command::new("cl.exe")
	.args(os::args()[1..].as_slice())
	.output(){
			Ok(output) => {
			println!("stdout: {}", String::from_utf8_lossy(output.output.as_slice()));
			println!("stderr: {}", String::from_utf8_lossy(output.error.as_slice()));
		}
			Err(e) => {
			panic!("{}", e);
		}
		}*/
}

fn parse_compilation_task(args: &[String]) -> Result<CompilationTask, String> {
	match parse_arguments(args) {
			Ok(parsed_args) => {
			// Source file name.
			let inputSource;
			match filter(&parsed_args, |arg:&Arg|->Option<Path>{
				match *arg {
						Arg::Input{ref kind, ref file, ..} if *kind == InputKind::Source => {Some(Path::new(file))}
						_ => {None}
					}
			}).as_slice() {
					[] => {
					return Err(format!("Can't find source file path."));
				}
					[ref v] => {
						inputSource = v.clone();
				}
					v => {
					return Err(format!("Found too many source files: {:?}", v));
				}
				};
			// Precompiled header file name.
			let inputPrecompiled;
			match filter(&parsed_args, |arg:&Arg|->Option<Path>{
				match *arg {
						Arg::Input{ref kind, ref file, ..} if *kind == InputKind::Precompiled => {Some(Path::new(file))}
						_ => {None}
					}
			}).as_slice() {
					[] => {
					inputPrecompiled=None;
				}
					[ref v] => {
						inputPrecompiled=Some(v.clone());
				}
					v => {
					return Err(format!("Found too many precompiled header files: {:?}", v));
				}
				};
			// Precompiled header marker name.
			let markerPrecompiled;
			match filter(&parsed_args, |arg:&Arg|->Option<String>{
				match *arg {
						Arg::Input{ref kind, ref file, ..} if *kind == InputKind::Marker => {Some(file.clone())}
						_ => {None}
					}
			}).as_slice() {
					[] => {
					markerPrecompiled=None;
				}
					[ref v] => {
						markerPrecompiled=Some(v.clone());
				}
					v => {
					return Err(format!("Found too many precompiled header markers: {:?}", v));
				}
				};
			// Precompiled header file name.
			let outputPrecompiled;
			match filter(&parsed_args, |arg:&Arg|->Option<Path>{
				match *arg {
						Arg::Output{ref kind, ref file, ..} if *kind == OutputKind::Precompiled => {Some(Path::new(file))}
						_ => {None}
					}
			}).as_slice() {
					[] => {
					outputPrecompiled=None;
				}
					[ref v] => {
						outputPrecompiled=Some(v.clone());
				}
					v => {
					return Err(format!("Found too many precompiled header output files: {:?}", v));
				}
				};
			// Output object file name.
			let outputObject;
			match filter(&parsed_args, |arg:&Arg|->Option<Path>{
				match *arg {
						Arg::Output{ref kind, ref file, ..} if *kind == OutputKind::Object => {Some(Path::new(file))}
						_ => {None}
					}
			}).as_slice() {
					[] => {
						outputObject = inputSource.with_extension("obj");
				}
					[ref v] => {
						outputObject = v.clone();
				}
					v => {
					return Err(format!("Found too many output object files: {:?}", v));
				}
				};
			// Language
			let language: String;
			match filter(&parsed_args, |arg:&Arg|->Option<String>{
				match arg {
						&Arg::Param{ref flag, ref value, ..} if *flag == "T" => {Some(value.clone())}
						_ => {None}
					}
			}).as_slice() {
					[]  => {
					match inputSource.extension_str() {
							Some(e) if e.eq_ignore_ascii_case("cpp") => {language = "P".to_string();}
							Some(e) if e.eq_ignore_ascii_case("c") => {language = "C".to_string();}
							_ => {
							return Err(format!("Can't detect file language by extension: {:?}", inputSource));
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
			args: parsed_args,
			language: language,
			inputSource: inputSource,
			inputPrecompiled: inputPrecompiled,
			outputObject: outputObject,
			outputPrecompiled: outputPrecompiled,
			markerPrecompiled: markerPrecompiled,
			})
		}
			Err(e) => {Err(e)}
		}
}

fn preprocess(task: &CompilationTask) {
	let mut args = filter(&task.args, |arg:&Arg|->Option<String> {
		match arg {
				&Arg::Flag{ref scope, ref flag} => {
				match scope {
						&Scope::Preprocessor | &Scope::Shared => {Some("/".to_string() + flag.as_slice())}
						&Scope::Ignore | &Scope::Compiler => {None}
					}
			}
				&Arg::Param{ref scope, ref  flag, ref value} => {
				match scope {
						&Scope::Preprocessor | &Scope::Shared => {Some("/".to_string() + flag.as_slice() + value.as_slice())}
						&Scope::Ignore | &Scope::Compiler => {None}
					}
			}
				&Arg::Input{..} => {None}
				&Arg::Output{..} => {None}
			}
	});
	args.push("/T".to_string() + task.language.as_slice());
	args.push("/E".to_string());
	args.push(task.inputSource.display().to_string());

	println!("Preprocess");
	println!(" - args: {:?}", args);
	match Command::new("cl.exe")
	.args(args.as_slice())
	.output(){
			Ok(output) => {
			println!("stdout: {}", String::from_utf8_lossy(match task.inputPrecompiled {
			Some(_) => {filter_precompiled(output.output.as_slice(), &task.markerPrecompiled, task.outputPrecompiled.is_some())}
			None => {output.output}
			}.as_slice()));
			println!("stderr: {}", String::from_utf8_lossy(output.error.as_slice()));
		}
			Err(e) => {
			panic!("{}", e);
		}
		}
}

const TAB: u8 = '\t' as u8;
const LN: u8 = '\n' as u8;
const LR: u8 = '\r' as u8;
const SPACE: u8 = ' ' as u8;
const SHARP: u8 = '#' as u8;

fn filter_precompiled(input: &[u8], marker: &Option<String>, keep_headers: bool) -> Vec<u8> {
	let mut result: Vec<u8> = Vec::new();
	let mut lineBegin = true;
	let mut skipHeader: bool = !keep_headers;
	let mut iter: Iter<u8> = input.iter();
	// Entry file.
	let mut entryFile: Option<String> = None;
	let mut headerFound: bool = false;
	loop {
		match iter.next() {
				Some(c) => {
				match *c {
						LN | LR => {
							result.push(*c);
							lineBegin = true;
					}
						TAB | SPACE => {
						if (!skipHeader) {
								result.push(*c);
						}
					}
						SHARP => {
						let directive = read_directive(&mut iter);
						match directive {
								Directive::Line(raw, line, file) => {

								entryFile = match entryFile {
										Some(path) => {
										if headerFound && (path  == file) {
											println! ("FOUND");
											result.push_all("#pragma hdrstop\n".as_bytes());
											result.push(*c);
											result.push_all(raw.as_slice());
											break;
										}
										match *marker {
												Some(ref markerFile) => {
												if (Path::new(file).ends_with_path(&Path::new(markerFile.as_slice()))) {
													headerFound = true;
													println! ("HEADER");
												}
											}
												None => {}
											}
										Some(path)
									}
										None => {
											Some(file)
									}
									};
									result.push(*c);
									result.push_all(raw.as_slice());
							}
								Directive::HdrStop(raw) => {
									result.push(*c);
									result.push_all(raw.as_slice());
									break;
							}
								Directive::Unknown(raw) => {
									result.push(*c);
									result.push_all(raw.as_slice());
							}
							}
						lineBegin = false;
					}
						_ => {
						if (!skipHeader) {
								result.push(*c);
						}
						lineBegin = false;}
					}
			}
				None => {
				break;
			}
			}
	}
	loop {
		match iter.next() {
				Some(c) => {
					result.push(c.clone());
			}
				_ => {
				break;
			}
			}
	}
	return result;
}

#[derive(Show)]
enum Directive {
// raw, line, file
Line(Vec<u8>, String, String),
// raw
HdrStop(Vec<u8>),
// raw
Unknown(Vec<u8>)
}

fn read_directive(iter: &mut Iter<u8>) -> Directive {
	let mut unknown: Vec<u8> = Vec::new();
	let (next, token) = read_token(None, iter, &mut unknown);
	match token.as_slice() {
			"line" => {
				read_directive_line(next, iter, unknown)
		}
			"pragma" => {
				read_directive_pragma(next, iter, unknown)
		}
			_ => {
				skip_line(next, iter, &mut unknown);
				Directive::Unknown(unknown)
		}
		}
}

fn read_directive_line(first: Option<u8>, iter: &mut Iter<u8>, mut unknown: Vec<u8>) -> Directive {
	let (next1, line) = read_token(first, iter, &mut unknown);
	let (next2, file) = read_token(next1, iter, &mut unknown);
	skip_line(next2, iter, &mut unknown);
	Directive::Line(unknown, line, file)
}

fn read_directive_pragma(first: Option<u8>, iter: &mut Iter<u8>, mut unknown: Vec<u8>) -> Directive {
	let (next, token) = read_token(first, iter, &mut unknown);
	skip_line(next, iter, &mut unknown);
	match token.as_slice() {
			"hdrstop" => {Directive::HdrStop(unknown)}
			_ => {Directive::Unknown(unknown)}
		}
}

fn skip_spaces(first: Option<u8>, iter: &mut Iter<u8>, unknown: &mut Vec<u8>) -> Option<u8> {
	match first {
			Some(c) => {
			match c {
					LN | LR => {return None;}
					TAB | SPACE => {}
					_ => {return first;}
				}
		}
			_ => {}
		}
	loop {
		match iter.next() {
				Some(c) => {
					unknown.push(*c);
					match c {
							&LN | &LR => {return None;}
							&TAB | &SPACE => {}
							_ => {return Some(*c);}
						}
			}
				None => {
				return None;
			}
			}
	}
}

fn skip_line(first: Option<u8>, iter: &mut Iter<u8>, unknown: &mut Vec<u8>) {
	match first {
			Some(c) => {
			match c {
					LN | LR => {return;}
					_ => {}
				}
		}
			_ => {}
		}
	loop {
		match iter.next() {
				Some(c) => {
					unknown.push(*c);
					match c {
							&LN | &LR => {return;}
							_ => {}
						}
			}
				None => {return;}
			}
	}
}

fn read_token(first: Option<u8>, iter: &mut Iter<u8>, unknown: &mut Vec<u8>) -> (Option<u8>, String) {
	match skip_spaces(first, iter, unknown) {
			Some(first_char) => {
			let mut token: Vec<u8> = Vec::new();
			let mut escape = false;
			let quote: bool;
			if first_char == '"' as u8 {
				quote = true;
			} else {
					token.push(first_char);
					quote = false;
			}
			loop {
				match iter.next() {
						Some(c) if quote => {
							unknown.push(*c);
							if (escape) {
								if *c == ('n' as u8) {
										token.push('\n' as u8);
								} else if *c == ('r' as u8) {
										token.push('\r' as u8);
								} else if *c == ('t' as u8) {
										token.push('\t' as u8);
								} else {
										token.push(*c);
								}
								escape = false;
							} else if (*c == ('\\' as u8)) {
								escape = true;
							} else if (*c == ('"' as u8)) {
								return (match iter.next() {
										Some(n) => {
											unknown.push(*n);
											Some(*n)
									}
										None => {None}
									}   , String::from_utf8_lossy(token.as_slice()).to_string());
							} else {
									token.push(*c);
							}
					}
						Some(c) => {
							unknown.push(*c);
							if ((*c >= ('a' as u8)) && (*c <= ('z' as u8))) ||
							((*c >= ('A' as u8)) && (*c <= ('Z' as u8))) ||
							((*c >= ('0' as u8)) && (*c <= ('9' as u8))) {
									token.push(*c);
							} else {
								return (Some(*c), String::from_utf8_lossy(token.as_slice()).to_string());
							}
					}
						None => {
						return (None, String::from_utf8_lossy(token.as_slice()).to_string());
					}
					}

			}
		}
			None => {
			return (None, String::new());
		}
		}
}

fn compile(task: &CompilationTask) {
	let mut args = filter(&task.args, |arg:&Arg|->Option<String> {
		match arg {
				&Arg::Flag{ref scope, ref flag} => {
				match scope {
						&Scope::Preprocessor | &Scope::Compiler | &Scope::Shared => {Some("/".to_string() + flag.as_slice())}
						&Scope::Ignore => {None}
					}
			}
				&Arg::Param{ref scope, ref  flag, ref value} => {
				match scope {
						&Scope::Preprocessor | &Scope::Compiler | &Scope::Shared => {Some("/".to_string() + flag.as_slice() + value.as_slice())}
						&Scope::Ignore => {None}
					}
			}
				&Arg::Input{..} => {None}
				&Arg::Output{..} => {None}
			}
	});
	args.push("/T".to_string() + task.language.as_slice());
	match &task.inputPrecompiled {
			&Some(ref path) => {args.push("/Fp".to_string() + path.display().to_string().as_slice());}
			&None => {}
		}
	args.push(task.inputSource.display().to_string() + ".i");

	args.push("/c".to_string());
	args.push("/Fo".to_string() + task.outputObject.display().to_string().as_slice());
	match &task.inputPrecompiled {
			&Some(ref path) => {args.push("/Fp".to_string() + path.display().to_string().as_slice());}
			&None => {}
		}
	match &task.outputPrecompiled {
			&Some(ref path) => {args.push("/Yc".to_string() + path.display().to_string().as_slice());}
			&None => {}
		}

	println!("Compile");
	println!(" - args: {:?}", args);
	match Command::new("cl.exe")
	.args(args.as_slice())
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

fn filter<T, R, F:Fn(&T) -> Option<R>>(args: &Vec<T>, filter:F) -> Vec<R> {
	let mut result: Vec<R> = Vec::new();
	for arg in args.iter() {
		match filter(arg) {
				Some(v) => {
					result.push(v);
			}
				None => {}
			}
	}
	result
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
										s if s.starts_with("Yc") => {
											Ok(Arg::Output{kind:OutputKind::Precompiled, flag:"Yc".to_string(), file:s[2..].to_string()})
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

#[test]
fn test_filter_precompiled_keep() {
	let filtered = filter_precompiled(r#"#line 1 "sample.cpp"
#line 1 "e:\\work\\octobuild\\test_cl\\sample header.h"
# pragma once
void hello();
#line 2 "sample.cpp"

int main(int argc, char **argv) {
	return 0;
}
"#.as_bytes(), &Some("sample header.h".to_string()), true);
	let result = String::from_utf8_lossy(filtered.as_slice());
	assert_eq!(result.as_slice(), r#"#line 1 "sample.cpp"
#line 1 "e:\\work\\octobuild\\test_cl\\sample header.h"
# pragma once
void hello();
#pragma hdrstop
#line 2 "sample.cpp"

int main(int argc, char **argv) {
	return 0;
}
"#);
}

#[test]
fn test_filter_precompiled_remove() {
	let filtered = filter_precompiled(r#"#line 1 "sample.cpp"
#line 1 "e:\\work\\octobuild\\test_cl\\sample header.h"
# pragma once
void hello1();
void hello2();
#line 2 "sample.cpp"

int main(int argc, char **argv) {
	return 0;
}
"#.as_bytes(), &Some("sample header.h".to_string()), false);
	let result = String::from_utf8_lossy(filtered.as_slice());
	assert_eq!(result.as_slice(), r#"#line 1 "sample.cpp"
#line 1 "e:\\work\\octobuild\\test_cl\\sample header.h"
# pragma once


#pragma hdrstop
#line 2 "sample.cpp"

int main(int argc, char **argv) {
	return 0;
}
"#);
}

#[test]
fn test_filter_precompiled_hdrstop() {
	let filtered = filter_precompiled(r#"#line 1 "sample.cpp"
 #line 1 "e:\\work\\octobuild\\test_cl\\sample header.h"
void hello();
# pragma  hdrstop
void data();
# pragma once
#line 2 "sample.cpp"

int main(int argc, char **argv) {
	return 0;
}
"#.as_bytes(), &None, false);
	let result = String::from_utf8_lossy(filtered.as_slice());
	assert_eq!(result.as_slice(), r#"#line 1 "sample.cpp"
#line 1 "e:\\work\\octobuild\\test_cl\\sample header.h"

# pragma  hdrstop
void data();
# pragma once
#line 2 "sample.cpp"

int main(int argc, char **argv) {
	return 0;
}
"#);
}
