pub use super::super::compiler::*;

use super::super::cache::Cache;
use super::postprocess;
use super::super::wincmd;
use super::super::utils::filter;
use super::super::utils::hash_text;
use super::super::io::tempfile::TempFile;

use std::fs::File;
use std::io::{Error, BufReader, Cursor, Read, Write};
use std::hash::{SipHasher, Hasher};
use std::path::{Path, PathBuf};
use std::process::Output;

pub struct VsCompiler {
	cache: Cache,
	temp_dir: PathBuf
}

impl VsCompiler {
	pub fn new(cache: &Cache, temp_dir: &Path) -> Self {
		VsCompiler {
			cache: cache.clone(),
			temp_dir: temp_dir.to_path_buf()
		}
	}
}

impl Compiler for VsCompiler {
	fn create_task(&self, command: CommandInfo, args: &[String]) -> Result<CompilationTask, String> {
		super::prepare::create_task(command, args)
	}

	fn preprocess_step(&self, task: &CompilationTask) -> Result<PreprocessResult, Error> {
		// Make parameters list for preprocessing.
		let mut args = filter(&task.args, |arg:&Arg|->Option<String> {
			match arg {
				&Arg::Flag{ref scope, ref flag} => {
					match scope {
						&Scope::Preprocessor | &Scope::Shared => Some("/".to_string() + &flag),
						&Scope::Ignore | &Scope::Compiler => None
					}
				}
				&Arg::Param{ref scope, ref  flag, ref value} => {
					match scope {
						&Scope::Preprocessor | &Scope::Shared => Some("/".to_string() + &flag + &value),
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
		args.push("/T".to_string() + &task.language);
		args.push("/P".to_string());
		args.push(task.input_source.display().to_string());
	
		// Hash data.
		let mut sip_hash = SipHasher::new();
		let hash: &mut Hasher = &mut sip_hash;
		hash.write(&[0]);
		hash.write(wincmd::join(&args).as_bytes());
	
		let mut command = task.command.to_command();
		command
			.args(&args)
			.arg(&join_flag("/Fi", &temp_file.path()))
			.arg(&join_flag("/Fo", &task.output_object)); // /Fo option also set output path for #import directive
		let output = try! (command.output());
		if output.status.success() {
			match File::open(temp_file.path()) {
				Ok(stream) => {
					let (mut output, sources): (Box<Read>, Vec<PathBuf>) = if task.input_precompiled.is_some() || task.output_precompiled.is_some() {
						let mut buffer: Vec<u8> = Vec::new();
						let sources = try! (postprocess::filter_preprocessed(&task.command.current_dir, &mut BufReader::new(stream), &mut buffer, &task.marker_precompiled, task.output_precompiled.is_some()));
						(Box::new(Cursor::new(buffer)), sources)
					} else {
						(Box::new(stream), Vec::new())
					};
					let mut content = Vec::new();
					try! (output.read_to_end(&mut content));
					hash.write(&content);
					Ok(PreprocessResult::Success(PreprocessedSource {
						hash: format!("{:016x}", hash.finish()),
						sources: sources,
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
		let mut args = filter(&task.args, |arg:&Arg|->Option<String> {
			match arg {
				&Arg::Flag{ref scope, ref flag} => {
					match scope {
						&Scope::Compiler | &Scope::Shared => Some("/".to_string() + &flag),
						&Scope::Preprocessor if task.output_precompiled.is_some() => Some("/".to_string() + &flag),
						&Scope::Ignore | &Scope::Preprocessor => None
					}
				}
				&Arg::Param{ref scope, ref  flag, ref value} => {
					match scope {
						&Scope::Compiler | &Scope::Shared => Some("/".to_string() + &flag + &value),
						&Scope::Preprocessor if task.output_precompiled.is_some() => Some("/".to_string() + &flag + &value),
						&Scope::Ignore | &Scope::Preprocessor => None
					}
				}
				&Arg::Input{..} => None,
				&Arg::Output{..} => None
			}
		});
		args.push("/T".to_string() + &task.language);
		match &task.input_precompiled {
			&Some(ref path) => {
				args.push("/Yu".to_string());
				args.push("/Fp".to_string() + &path.display().to_string());
			}
			&None => {}
		}
		if task.output_precompiled.is_some() {
			args.push("/Yc".to_string());
		}
		// Input data, stored in files.
		let mut inputs: Vec<PathBuf> = Vec::new();
		match &task.input_precompiled {
				&Some(ref path) => {inputs.push(path.clone());}
				&None => {}
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
	
		let hash_params = hash_text(&preprocessed.content) + &wincmd::join(&args);
		self.cache.run_cached(&hash_params, &inputs, &outputs, || -> Result<OutputInfo, Error> {
			// Input file path.
			let input_temp = TempFile::new_in(&self.temp_dir, ".i");
			try! (try! (File::create(input_temp.path())).write_all(&preprocessed.content));
			// Run compiler.
			let mut command = task.command.to_command();
			command
				.args(&args)
				.arg(input_temp.path().to_str().unwrap())
				.arg("/c")
				.arg(&join_flag("/Fo", &task.output_object));
			match &task.input_precompiled {
				&Some(ref path) => {command.arg(&join_flag("/Fp", path));}
				&None => {}
			}
			match &task.output_precompiled {
				&Some(ref path) => {command.arg(&join_flag("/Fp", path));}
				&None => {}
			}		
			command.output().map(|o| OutputInfo::new(o))
		})
	}
}

pub fn join_flag(flag: &str, path: &Path) -> String {
	flag.to_string() + &path.to_str().unwrap()
}