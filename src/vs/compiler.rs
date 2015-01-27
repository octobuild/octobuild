pub use super::super::compiler::Compiler;
pub use super::super::compiler::{Arg, CompilationTask, PreprocessResult, Scope};

use super::super::cache::Cache;
use super::postprocess;
use super::super::wincmd;
use super::super::utils::filter;
use super::super::utils::hash_text;
use super::super::io::tempfile::TempFile;

use std::io::{Command, File, IoError, IoErrorKind};
use std::io::process::ProcessOutput;
use std::hash::SipHasher;

pub struct VsCompiler {
	cache: Cache,
	temp_dir: Path
}

impl VsCompiler {
	pub fn new(cache: &Cache, temp_dir: &Path) -> Self {
		VsCompiler {
			cache: cache.clone(),
			temp_dir: temp_dir.clone()
		}
	}
}

impl Compiler for VsCompiler {
	fn create_task(&self, command: &Command, args: &[String]) -> Result<CompilationTask, String> {
		super::prepare::create_task(command, args)
	}

	fn preprocess_step(&self, task: &CompilationTask) -> Result<PreprocessResult, IoError> {
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
		let temp_file = TempFile::new_in(&self.temp_dir, ".i");
		args.push("/nologo".to_string());
		args.push("/T".to_string() + task.language.as_slice());
		args.push("/P".to_string());
		args.push(task.input_source.display().to_string());
	
		// Hash data.
		let mut hash = SipHasher::new();
		{
			use std::hash::Writer;
			hash.write(&[0]);
			hash.write(wincmd::join(&args).as_bytes());
		}
	
		let mut command = task.command.clone();
		command
			.args(args.as_slice())
			.arg("/Fi".to_string() + temp_file.path().display().to_string().as_slice());
		let output = try! (command.output());
		if output.status.success() {
			match File::open(temp_file.path()).read_to_end() {
				Ok(content) => {
					let output = if task.input_precompiled.is_some() || task.output_precompiled.is_some() {
						match postprocess::filter_preprocessed(content.as_slice(), &task.marker_precompiled, task.output_precompiled.is_some()) {
							Ok(output) => output,
							Err(e) => {
								return Err(IoError {
									kind: IoErrorKind::InvalidInput,
									desc: "Can't parse preprocessed file",
									detail: Some(e)
								});
							}
						}
					} else {
						content
					};
					{
						use std::hash::Writer;
						hash.write(output.as_slice());
					}
					Ok(PreprocessResult{
						hash: format!("{:016x}", hash.result()),
						content: output
					})
				}
				Err(e) => Err(e)
			}
		} else {
			Err(IoError {
				kind: IoErrorKind::IoUnavailable,
				desc: "Invalid preprocessor exit code with parameters",
				detail: Some(format!("{:?}", args))
			})
		}
	}

	// Compile preprocessed file.
	fn compile_step(&self, task: &CompilationTask, preprocessed: PreprocessResult) -> Result<ProcessOutput, IoError> {
		let mut args = filter(&task.args, |arg:&Arg|->Option<String> {
			match arg {
				&Arg::Flag{ref scope, ref flag} => {
					match scope {
						&Scope::Compiler | &Scope::Shared => Some("/".to_string() + flag.as_slice()),
						&Scope::Ignore | &Scope::Preprocessor => None
					}
				}
				&Arg::Param{ref scope, ref  flag, ref value} => {
					match scope {
						&Scope::Compiler | &Scope::Shared => Some("/".to_string() + flag.as_slice() + value.as_slice()),
						&Scope::Ignore | &Scope::Preprocessor => None
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
	
		let hash_params = hash_text(preprocessed.content.as_slice()) + wincmd::join(&args).as_slice();
		self.cache.run_cached(hash_params.as_slice(), &inputs, &outputs, || -> Result<ProcessOutput, IoError> {
			// Input file path.
			let input_temp = TempFile::new_in(&self.temp_dir, ".i");
			try! (File::create(input_temp.path()).write(preprocessed.content.as_slice()));
			// Run compiler.
			let mut command = task.command.clone();
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
			command.output()
		})
	}
}
