#![allow(unstable)]
extern crate octobuild;
extern crate log;

use octobuild::wincmd;
use std::ascii::AsciiExt;

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
							_ => {return Err(format!("Unknown source language type: {}", v));}
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
	match &task.inputPrecompiled {
			&Some(ref path) => {args.push("/Fp".to_string() + path.display().to_string().as_slice());}
			&None => {}
		}
	args.push(task.inputSource.display().to_string());

	args.push("/P".to_string());
	args.push("/Fi".to_string() + task.inputSource.display().to_string().as_slice() + ".i");

	println!("Preprocess");
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
	match &task.markerPrecompiled {
			&Some(ref marker) => {args.push("/Yu".to_string() + marker.as_slice());}
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