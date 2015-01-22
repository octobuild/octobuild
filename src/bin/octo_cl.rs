#![allow(unstable)]
extern crate octobuild;
extern crate log;
extern crate "sha1-hasher" as sha1;

use octobuild::wincmd;
use octobuild::io::tempfile::TempFile;
use octobuild::vs::compiler::VsCompiler;
use octobuild::compiler::Compiler;
use octobuild::utils::filter;
use std::ascii::AsciiExt;
use std::str::StrExt;

use std::os;
use std::slice::{Iter};

use std::io::{Command, File, IoError, IoErrorKind, TempDir};

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
	let temp_dir = match TempDir::new("octobuild") {
		Ok(result) => result,
		Err(e) => {panic!(e);}
	};
	println!("Parsed task: {:?}", result);
	match result {
			Ok(task) => {
				match preprocess(&temp_dir, &task) {
					Ok(result) => {
						compile(&temp_dir, &task, result);
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

fn preprocess(temp_dir: &TempDir, task: &CompilationTask) -> Result<PreprocessResult, String> {
	// Make parameters list for preprocessing.
	let mut args = filter(&task.args, |arg:&Arg|->Option<String> {
		match arg {
			&Arg::Flag{ref scope, ref flag} => {
				match scope {
					&Scope::Preprocessor | &Scope::Shared => Some("/".to_string() + flag.as_slice()),
					&Scope::Ignore | &Scope::Compiler => None
				}
			}
			&Arg::Param{ref scope, ref  flag, ref value} => {
				match scope {
					&Scope::Preprocessor | &Scope::Shared => Some("/".to_string() + flag.as_slice() + value.as_slice()),
					&Scope::Ignore | &Scope::Compiler => None
				}
			}
			&Arg::Input{..} => None,
			&Arg::Output{..} => None,
		}
	});

  // Add preprocessor paramters.
	let temp_file = TempFile::new_in(temp_dir.path(), ".i");
	args.push("/nologo".to_string());
	args.push("/T".to_string() + task.language.as_slice());
	args.push("/P".to_string());
	args.push(task.input_source.display().to_string());

	// Hash data.
	let mut hash = sha1::Sha1::new();
	{
		use std::hash::Writer;
		hash.write(&[0]);
		hash.write(wincmd::join(&args).as_bytes());
	}

	args.push("/Fi".to_string() + temp_file.path().display().to_string().as_slice());

	let compiler:VsCompiler = Compiler::new();
	println!("Preprocess");
	println!(" - args: {}", wincmd::join(&args));
	match Command::new("cl.exe")
	.args(args.as_slice())
	.output(){
			Ok(output) => {
			println!("stderr: {}", String::from_utf8_lossy(output.error.as_slice()));
			if output.status.success() {
						let result = match File::open(temp_file.path()).read_to_end() {
								Ok(content) => {
								match compiler.filter_preprocessed(content.as_slice(), &task.marker_precompiled, task.output_precompiled.is_some()) {
										Ok(output) => {
										{
											use std::hash::Writer;
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
					result
				} else {
					Err(format!("Preprocessing command with arguments failed: {:?}", args))
			}
		}
			Err(e) => {Err(e.to_string())}
		}
}


fn compile(temp_dir: &TempDir, task: &CompilationTask, preprocessed: PreprocessResult) {
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

	if task.output_precompiled.is_some() {
		args.push("/Yc".to_string());
	}

	let cache_path: Path;
	{
		use std::hash::Writer;
		let mut hash = sha1::Sha1::new();
		hash.write(wincmd::join(&args).as_bytes());
		hash.write(&[0]);
		hash.write(preprocessed.hash.as_bytes());
		hash.write(&[0]);
		match &task.input_precompiled {
			&Some(ref path) => {
				match File::open(path).read_to_end() {
					Ok(content) => {
						hash.write(content.as_slice());
					}
					Err(e) => {
						return;
					}
				}
			}
			&None => {}
		}

		cache_path = Path::new(".".to_string() + hash.hexdigest().as_slice());
	}

	match File::open(&cache_path) {
		Ok(mut file) => {
			if extract_cache(&mut file, &Some(task.output_object.clone())).is_ok() &&
				extract_cache(&mut file, &task.output_precompiled).is_ok() {
				return;
			}
		}
		Err(_) => {
		}
	}

	// Input file path.
	let input_temp = TempFile::new_in(temp_dir.path(), ".i");
	match File::create(input_temp.path()).write(preprocessed.content.as_slice()) {
		Ok(()) => {}
		Err(e) => {panic!(e);}
	}
	args.push(input_temp.path().display().to_string());

	args.push("/c".to_string());
	args.push("/Fo".to_string() + task.output_object.display().to_string().as_slice());
	match &task.input_precompiled {
			&Some(ref path) => {args.push("/Fp".to_string() + path.display().to_string().as_slice());}
			&None => {}
		}
	match &task.output_precompiled {
			&Some(ref path) => {
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

			match File::create(&cache_path) {
				Ok(mut file) => {
					write_cache(&mut file, &Some(task.output_object.clone()));
					write_cache(&mut file, &task.output_precompiled);
				}
				Err(e) => {}
			}

			println!("stdout: {}", String::from_utf8_lossy(output.output.as_slice()));
			println!("stderr: {}", String::from_utf8_lossy(output.error.as_slice()));
		}
			Err(e) => {
			panic!("{}", e);
		}
		}
}

const VERSION: u8 = 1;
const MAGIC: u32 = 0x1e457b89;

fn write_cache(cache: &mut File, source: &Option<Path>) -> Result<(), IoError> {
	match source {
		&Some(ref path) => {
			match File::open(path).read_to_end() {
				Ok(content) => {
					try! (cache.write_u8(VERSION));
					try! (cache.write_le_u32(content.len() as u32));
					try! (cache.write(content.as_slice()));
					cache.write_le_u32(MAGIC)
				}
				Err(e) => Err(e)
			}
		}
		&None => {
			Ok(())
		}
	}
}

fn extract_cache(cache: &mut File, target: &Option<Path>) -> Result<(), IoError> {
	let path = match target {
		&Some(ref path) => path,
		&None => return Ok(())
	};
	// Check version.
	match cache.read_u8() {
		Ok(mark) if mark == VERSION => (),
		Ok(_) => return Err(IoError {
			kind: IoErrorKind::InvalidInput,
			desc: "Unexpected file data",
			detail: None
		}),
		Err(e) => return Err(e)
	};
	// Read content.
	let size = try! (cache.read_le_u32());
	// Read content.
	let content = try! (cache.read_exact(size as usize));
	// Check magic.
	match cache.read_le_u32() {
		Ok(mark) if mark == MAGIC => (),
		Ok(_) => return Err(IoError {
			kind: IoErrorKind::InvalidInput,
			desc: "Unexpected file data",
			detail: None
		}),
		Err(e) => return Err(e)
	};
	// Write result.
	match File::create(path).write(content.as_slice()) {
		Ok(_) => Ok(()),
		Err(e) => Err(e)
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
