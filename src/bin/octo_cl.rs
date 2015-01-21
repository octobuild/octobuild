#![allow(unstable)]
extern crate octobuild;
extern crate log;
extern crate "sha1-hasher" as sha1;

use octobuild::wincmd;
use std::ascii::AsciiExt;
use std::str::StrExt;

use std::os;
use std::slice::{Iter};

use std::io::fs;
use std::io::{Command, File};

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
Marker,
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
input_source: Path,
// Input precompiled header file name.
input_precompiled: Option<Path>,
// Output object file name.
output_object: Path,
// Output precompiled header file name.
output_precompiled: Option<Path>,
// Marker for precompiled header.
marker_precompiled: Option<String>,
}

struct PreprocessResult {
// Hash
hash: String,
// Preprocessed file
content: Vec<u8>,
}

fn main() {
	let result = parse_compilation_task(&os::args()[1..]);
	println!("Parsed task: {:?}", result);
	match result {
			Ok(task) => {
				match preprocess(&task) {
					Ok(result) => {
						compile(&task, result);
					}
					Err(e) => {
							panic!(e);
					}
					}
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
			let input_source;
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
						input_source = v.clone();
				}
					v => {
					return Err(format!("Found too many source files: {:?}", v));
				}
				};
			// Precompiled header file name.
			let precompiled_file;
			match filter(&parsed_args, |arg:&Arg|->Option<Path>{
				match *arg {
						Arg::Input{ref kind, ref file, ..} if *kind == InputKind::Precompiled => {Some(Path::new(file))}
						_ => {None}
					}
			}).as_slice() {
					[] => {
					precompiled_file=None;
				}
					[ref v] => {
							precompiled_file=Some(v.clone());
				}
					v => {
					return Err(format!("Found too many precompiled header files: {:?}", v));
				}
				};
			// Precompiled header file name.
			let marker_precompiled;
			let input_precompiled;
			let output_precompiled;
			match filter(&parsed_args, |arg:&Arg|->Option<(bool, String)>{
				match *arg {
						Arg::Input{ref kind, ref file, ..} if *kind == InputKind::Marker => {Some((true, file.clone()))}
						Arg::Output{ref kind, ref file, ..} if *kind == OutputKind::Marker => {Some((false, file.clone()))}
						_ => {None}
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
			let output_object;
			match filter(&parsed_args, |arg:&Arg|->Option<Path>{
				match *arg {
						Arg::Output{ref kind, ref file, ..} if *kind == OutputKind::Object => {Some(Path::new(file))}
						_ => {None}
					}
			}).as_slice() {
					[] => {
						output_object = input_source.with_extension("obj");
				}
					[ref v] => {
						output_object = v.clone();
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

fn preprocess(task: &CompilationTask) -> Result<PreprocessResult, String> {
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

	let temp_file = Path::new(task.input_source.display().to_string() + ".i~");
	args.push("/nologo".to_string());
	args.push("/T".to_string() + task.language.as_slice());
	args.push("/P".to_string());
	args.push(task.input_source.display().to_string());
	let args_hash = wincmd::join(&args);
	args.push("/Fi".to_string() + temp_file.display().to_string().as_slice());

	println!("Preprocess");
	println!(" - args: {}", wincmd::join(&args));
	match Command::new("cl.exe")
	.args(args.as_slice())
	.output(){
			Ok(output) => {
			println!("stderr: {}", String::from_utf8_lossy(output.error.as_slice()));
			if output.status.success() {
						let result = match File::open(&temp_file).read_to_end() {
								Ok(content) => {
								match filter_precompiled(content.as_slice(), &task.marker_precompiled, task.output_precompiled.is_some()) {
										Ok(output) => {
										let mut hash = sha1::Sha1::new();
										{
											use std::hash::Writer;
											hash.write(args_hash.as_bytes());
											hash.write(&[0]);
											hash.write(output.as_slice());
										}
										println!("Hash: {}", hash.hexdigest());
										Ok(PreprocessResult{
										hash: hash.hexdigest(),
										content: output
										})
									}
										Err(e) => {
											Err(e)
									}
									}
							}
								Err(e) => {
									Err(e.to_string())
							}
							};
						fs::unlink(&temp_file);
					result
				} else {
					fs::unlink(&temp_file);
					Err(format!("Preprocessing command with arguments failed: {:?}", args))
			}
		}
			Err(e) => {Err(e.to_string())}
		}
}

fn filter_precompiled(input: &[u8], marker: &Option<String>, keep_headers: bool) -> Result<Vec<u8>, String> {
	let mut result: Vec<u8> = Vec::new();
	let mut line_begin = true;
	let mut iter: Iter<u8> = input.iter();
	// Entry file.
	let mut entry_file: Option<String> = None;
	let mut header_found: bool = false;
	loop {
		match iter.next() {
				Some(c) => {
				match *c {
						b'\n' | b'\r' => {
							result.push(*c);
							line_begin = true;
					}
						b'\t' | b' ' => {
						if keep_headers {
								result.push(*c);
						}
					}
						b'#' if line_begin => {
						let directive = read_directive(&mut iter);
						match directive {
								Directive::Line(raw, file) => {

								entry_file = match entry_file {
										Some(path) => {
										if header_found && (path  == file) {
												result.push_all(b"#pragma hdrstop\n");
												result.push(*c);
												result.push_all(raw.as_slice());
												break;
										}
										match *marker {
												Some(ref path) => {
												if Path::new(file).ends_with_path(&Path::new(path.as_slice())) {
													header_found = true;
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
					}
						_ => {
						if keep_headers {
								result.push(*c);
						}
						line_begin = false;
					}
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
	match marker {
			&Some(ref path) if !header_found => {
			return Err(format!("Can't find marker in preprocessed file: {}", path));
		}
			_ => {Ok(result)}
		}
}

#[derive(Show)]
enum Directive {
// raw, file
Line(Vec<u8>, String),
// raw
HdrStop(Vec<u8>),
// raw
Unknown(Vec<u8>)
}

fn read_directive(iter: &mut Iter<u8>) -> Directive {
	let mut unknown: Vec<u8> = Vec::new();
	let (next, token) = read_token(None, iter, &mut unknown);
	match token.as_slice() {
			b"line" => {
				read_directive_line(next, iter, unknown)
		}
			b"pragma" => {
				read_directive_pragma(next, iter, unknown)
		}
			_ => {
				skip_line(next, iter, &mut unknown);
				Directive::Unknown(unknown)
		}
		}
}

fn read_directive_line(first: Option<u8>, iter: &mut Iter<u8>, mut unknown: Vec<u8>) -> Directive {
	// Line number
	let (next1, _) = read_token(first, iter, &mut unknown);
	// File name
	let (next2, file) = read_token(next1, iter, &mut unknown);
	skip_line(next2, iter, &mut unknown);
	Directive::Line(unknown, String::from_utf8_lossy(file.as_slice()).to_string())
}

fn read_directive_pragma(first: Option<u8>, iter: &mut Iter<u8>, mut unknown: Vec<u8>) -> Directive {
	let (next, token) = read_token(first, iter, &mut unknown);
	skip_line(next, iter, &mut unknown);
	match token.as_slice() {
			b"hdrstop" => {Directive::HdrStop(unknown)}
			_ => {Directive::Unknown(unknown)}
		}
}

fn skip_spaces(first: Option<u8>, iter: &mut Iter<u8>, unknown: &mut Vec<u8>) -> Option<u8> {
	match first {
			Some(c) => {
			match c {
					b'\n' | b'\r' => {return None;}
					b'\t' | b' ' => {}
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
							&b'\n' | &b'\r' => {return None;}
							&b'\t' | &b' ' => {}
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
					b'\n' | b'\r' => {return;}
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
							&b'\n' | &b'\r' => {return;}
							_ => {}
							}
			}
				None => {return;}
			}
	}
}

fn read_token(first: Option<u8>, iter: &mut Iter<u8>, unknown: &mut Vec<u8>) -> (Option<u8>, Vec<u8>) {
	match skip_spaces(first, iter, unknown) {
			Some(first_char) => {
			let mut token: Vec<u8> = Vec::new();
			let mut escape = false;
			let quote: bool;
			if first_char == b'"' {
				quote = true;
			} else {
					token.push(first_char);
					quote = false;
			}
			loop {
				match iter.next() {
						Some(c) if quote => {
							unknown.push(*c);
							if escape {
								match *c {
									b'n' => token.push(b'\n'),
									b'r' => token.push(b'\r'),
									b't' => token.push(b'\t'),
									v => token.push(v)
								}
								escape = false;
							} else if *c == ('\\' as u8) {
								escape = true;
							} else if *c == b'"' {
								return (match iter.next() {
										Some(n) => {
											unknown.push(*n);
											Some(*n)
									}
										None => {None}
									}   , token);
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
								return (Some(*c), token);
							}
					}
						None => {
						return (None, token);
					}
					}

			}
		}
			None => {
			return (None, Vec::new());
		}
		}
}

fn compile(task: &CompilationTask, preprocessed: PreprocessResult) {
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
	match &task.input_precompiled {
			&Some(ref path) => {
				args.push("/Yu".to_string());
				args.push("/Fp".to_string() + path.display().to_string().as_slice());
			}
			&None => {}
		}

	// Input file path.
	let input_temp = Path::new(task.input_source.display().to_string()+".i");
	match File::create(&input_temp).write(preprocessed.content.as_slice()) {
	Ok(()) => {}
	Err(e) => {panic!(e);}
	}
	args.push(input_temp.display().to_string());

	args.push("/c".to_string());
	args.push("/Fo".to_string() + task.output_object.display().to_string().as_slice());
	match &task.input_precompiled {
			&Some(ref path) => {args.push("/Fp".to_string() + path.display().to_string().as_slice());}
			&None => {}
		}
	match &task.output_precompiled {
			&Some(ref path) => {
				args.push("/Yc".to_string());
				args.push("/Fp".to_string() + path.display().to_string().as_slice());
			}
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

	fs::unlink(&input_temp);
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
										"c" => {
											Ok(Arg::Flag{scope: Scope::Ignore, flag:flag.to_string()})
									}
										"bigobj" | "nologo" => {
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
											Ok(Arg::Output{kind:OutputKind::Marker, flag:"Yc".to_string(), file:s[2..].to_string()})
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
	let filtered = filter_precompiled(br#"#line 1 "sample.cpp"
#line 1 "e:/work/octobuild/test_cl/sample header.h"
# pragma once
void hello();
#line 2 "sample.cpp"

int main(int argc, char **argv) {
	return 0;
}
"#, &Some("sample header.h".to_string()), true);
	assert_eq!(String::from_utf8_lossy(filtered.unwrap().as_slice()), r#"#line 1 "sample.cpp"
#line 1 "e:/work/octobuild/test_cl/sample header.h"
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
	let filtered = filter_precompiled(br#"#line 1 "sample.cpp"
#line 1 "e:/work/octobuild/test_cl/sample header.h"
# pragma once
void hello1();
void hello2();
#line 2 "sample.cpp"

int main(int argc, char **argv) {
	return 0;
}
"#, &Some("sample header.h".to_string()), false);
	assert_eq!(String::from_utf8_lossy(filtered.unwrap().as_slice()), r#"#line 1 "sample.cpp"
#line 1 "e:/work/octobuild/test_cl/sample header.h"
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
	let filtered = filter_precompiled(br#"#line 1 "sample.cpp"
 #line 1 "e:/work/octobuild/test_cl/sample header.h"
void hello();
# pragma  hdrstop
void data();
# pragma once
#line 2 "sample.cpp"

int main(int argc, char **argv) {
	return 0;
}
"#, &None, false);
	assert_eq!(String::from_utf8_lossy(filtered.unwrap().as_slice()), r#"#line 1 "sample.cpp"
#line 1 "e:/work/octobuild/test_cl/sample header.h"

# pragma  hdrstop
void data();
# pragma once
#line 2 "sample.cpp"

int main(int argc, char **argv) {
	return 0;
}
"#);
}
