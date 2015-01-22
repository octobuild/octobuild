#![allow(unstable)]
extern crate octobuild;
extern crate log;
extern crate "sha1-hasher" as sha1;

use octobuild::wincmd;
use octobuild::io::tempfile::TempFile;
use octobuild::vs::compiler::VsCompiler;
use octobuild::compiler::Compiler;
use octobuild::compiler::{Arg, InputKind, OutputKind, Scope, CompilationTask, PreprocessResult};
use octobuild::utils::filter;
use std::ascii::AsciiExt;
use std::str::StrExt;

use std::os;
use std::slice::{Iter};

use std::io::{Command, File, IoError, IoErrorKind, TempDir};

fn main() {
	let temp_dir = match TempDir::new("octobuild") {
		Ok(result) => result,
		Err(e) => {panic!(e);}
	};
	let compiler = VsCompiler::new(temp_dir.path());
	let result = compiler.create_task(&os::args()[1..]);
	println!("Parsed task: {:?}", result);
	match result {
			Ok(task) => {
				match compiler.preprocess(&task) {
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
