#![allow(unstable)]
extern crate octobuild;
extern crate log;
extern crate "sha1-hasher" as sha1;

use octobuild::wincmd;
use octobuild::cache::Cache;
use octobuild::io::tempfile::TempFile;
use octobuild::vs::compiler::VsCompiler;
use octobuild::compiler::Compiler;
use octobuild::compiler::{Arg, Scope, CompilationTask, PreprocessResult};
use octobuild::utils::{filter, hash_sha1};
use std::os;

use std::io::{Command, File, IoError, TempDir};

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



fn compile(temp_dir: &TempDir, task: &CompilationTask, preprocessed: PreprocessResult) -> Result<(), IoError> {
	let cache: Cache = Cache::new();
	let mut args = filter(&task.args, |arg:&Arg|->Option<String> {
		match arg {
			&Arg::Flag{ref scope, ref flag} => {
				match scope {
					&Scope::Preprocessor | &Scope::Compiler | &Scope::Shared => Some("/".to_string() + flag.as_slice()),
					&Scope::Ignore => None
				}
			}
			&Arg::Param{ref scope, ref  flag, ref value} => {
				match scope {
					&Scope::Preprocessor | &Scope::Compiler | &Scope::Shared => Some("/".to_string() + flag.as_slice() + value.as_slice()),
					&Scope::Ignore => None
				}
			}
			&Arg::Input{..} => None,
			&Arg::Output{..} => None
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
	// Input data, stored in files.
	let mut inputs: Vec<Path> = Vec::new();
	match &task.input_precompiled {
			&Some(ref path) => {inputs.push(path.clone());}
			&None => {}
		}
	// Output files.
	let mut outputs: Vec<Path> = Vec::new();
	outputs.push(task.output_object.clone());
	match &task.output_precompiled {
		&Some(ref path) => {outputs.push(path.clone());}
		&None => {}
	}

	let hash_params = hash_sha1(preprocessed.content.as_slice()) + wincmd::join(&args).as_slice();
	cache.run_cached(hash_params.as_slice(), &inputs, &outputs, || -> Result<(), IoError> {
		// Input file path.
		let input_temp = TempFile::new_in(temp_dir.path(), ".i");
		try! (File::create(input_temp.path()).write(preprocessed.content.as_slice()));
		// Run compiler.
		let mut command = Command::new("cl.exe");
		command
			.args(args.as_slice())
			.arg(input_temp.path().display().to_string())
			.arg("/c".to_string())
			.arg("/Fo".to_string() + task.output_object.display().to_string().as_slice());
		match &task.input_precompiled {
			&Some(ref path) => {command.arg("/Fp".to_string() + path.display().to_string().as_slice());}
			&None => {}
		}
		match &task.output_precompiled {
			&Some(ref path) => {command.arg("/Fp".to_string() + path.display().to_string().as_slice());}
			&None => {}
		}
	
		let output = try! (command.output());
		println!("stdout: {}", String::from_utf8_lossy(output.output.as_slice()));
		println!("stderr: {}", String::from_utf8_lossy(output.error.as_slice()));

		Ok(())
	})
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
