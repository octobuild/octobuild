pub use super::super::compiler::*;

use super::super::cache::Cache;
use super::super::io::tempfile::TempFile;

use std::fs::File;
use std::io::{Error, Read, Write};
use std::hash::{SipHasher, Hasher};
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};

pub struct ClangCompiler {
	cache: Cache,
	temp_dir: PathBuf
}

impl ClangCompiler {
	pub fn new(cache: &Cache, temp_dir: &Path) -> Self {
		ClangCompiler {
			cache: cache.clone(),
			temp_dir: temp_dir.to_path_buf()
		}
	}
}

impl Compiler for ClangCompiler {
	fn create_task(&self, command: CommandInfo, args: &[String]) -> Result<CompilationTask, String> {
		super::prepare::create_task(command, args)
	}

	fn preprocess_step(&self, task: &CompilationTask) -> Result<PreprocessResult, Error> {
		let mut args = Vec::new();
		args.push("-E".to_string());
		args.push("-x".to_string());
		args.push(task.language.clone());
		args.push("-frewrite-includes".to_string());

		// Make parameters list for preprocessing.
		for arg in task.args.iter() {
			match arg {
				&Arg::Flag{ref scope, ref flag} => {
					match scope {
						&Scope::Preprocessor | &Scope::Shared => {
							args.push("-".to_string() + &flag);
						}
						&Scope::Ignore | &Scope::Compiler => {}
					}
				}
				&Arg::Param{ref scope, ref  flag, ref value} => {
					match scope {
						&Scope::Preprocessor | &Scope::Shared => {
							args.push("-".to_string() + &flag);
							args.push(value.clone());
						}
						&Scope::Ignore | &Scope::Compiler => {}
					}
				}
				&Arg::Input{..} => {}
				&Arg::Output{..} => {}
			};
		}
	
		// Add preprocessor paramters.
		let temp_file = TempFile::new_in(&self.temp_dir, ".i");
		args.push(task.input_source.display().to_string());
		args.push("-o".to_string());
		args.push(temp_file.path().display().to_string());
	
		// Hash data.
		let mut hash = SipHasher::new();
		hash.write(&[0]);
		hash_args(&mut hash, &args);
	
		let mut command = task.command.to_command();
		command
			.args(&args);
		let output = try! (command.output());
		if output.status.success() {
			match File::open(temp_file.path()) {
				Ok(mut stream) => {
					let mut content = Vec::new();
					try! (stream.read_to_end(&mut content));
					hash.write(&content);
					Ok(PreprocessResult::Success(PreprocessedSource {
						hash: format!("{:016x}", hash.finish()),
						content: content,
					}))
				}
				Err(e) => Err(e)
			}
		} else {
			Ok(PreprocessResult::Failed(OutputInfo::new(output)))
		}
	}

	// Compile preprocessed file.
	fn compile_step(&self, task: &CompilationTask, preprocessed: PreprocessedSource) -> Result<OutputInfo, Error> {
		let mut args = Vec::new();
		args.push("-c".to_string());
		args.push("-x".to_string());
		args.push(task.language.clone());
		for arg in task.args.iter() {
			match arg {
				&Arg::Flag{ref scope, ref flag} => {
					match scope {
						&Scope::Compiler | &Scope::Shared => {
							args.push("-".to_string() + &flag);
						}
						&Scope::Ignore | &Scope::Preprocessor => {}
					}
				}
				&Arg::Param{ref scope, ref  flag, ref value} => {
					match scope {
						&Scope::Compiler | &Scope::Shared => {
							args.push("-".to_string() + &flag);
							args.push(value.clone());
						}
						&Scope::Ignore | &Scope::Preprocessor => {}
					}
				}
				&Arg::Input{..} => {}
				&Arg::Output{..} => {}
			};
		}
		// Output files.
		let mut outputs: Vec<PathBuf> = Vec::new();
		outputs.push(task.output_object.clone());
		match &task.output_precompiled {
			&Some(ref path) => {
				outputs.push(path.clone());
			}
			&None => {}
		}

		let mut hash = SipHasher::new();
		hash.write(&preprocessed.content);
		hash_args(&mut hash, &args);
		self.cache.run_file_cached(hash.finish(), &Vec::new(), &outputs, || -> Result<OutputInfo, Error> {
			// Run compiler.
			let mut command = task.command.to_command();
			command
				.args(&args)
				.arg("-".to_string())
				.arg("-o".to_string())
				.arg(task.output_object.display().to_string());
			command
				.stdin(Stdio::piped())
				.spawn()
				.and_then(|mut child| {
					try! (child.stdin.as_mut().unwrap().write_all(&preprocessed.content));
					child.wait_with_output()
				})
				.map(|o| OutputInfo::new(o))
		}, || true)
	}
}

fn hash_args(hash: &mut Hasher, args: &Vec<String>) {
	hash.write(&[0]);
	for arg in args.iter() {
		hash.write_usize(arg.len());
		hash.write(&arg.as_bytes());
	}
	hash.write_isize(-1);
}

pub fn join_flag(flag: &str, path: &Path) -> String {
	flag.to_string() + &path.to_str().unwrap()
}