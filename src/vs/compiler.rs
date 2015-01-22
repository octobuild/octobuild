extern crate "sha1-hasher" as sha1;

pub use super::super::compiler::Compiler;
pub use super::super::compiler::{Arg, CompilationTask, PreprocessResult, Scope};

use super::postprocess;
use super::super::wincmd;
use super::super::utils::filter;
use super::super::io::tempfile::TempFile;

use std::io::{Command, File, IoError, IoErrorKind};

pub struct VsCompiler {
	temp_dir: Path
}

impl VsCompiler {
	pub fn new(temp_dir: &Path) -> Self {
		VsCompiler {
			temp_dir: temp_dir.clone()
		}
	}
}

impl Compiler for VsCompiler {
	fn preprocess(&self, task: &CompilationTask) -> Result<PreprocessResult, IoError> {
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
		let mut hash = sha1::Sha1::new();
		{
			use std::hash::Writer;
			hash.write(&[0]);
			hash.write(wincmd::join(&args).as_bytes());
		}
	
		println!("Preprocess");
		println!(" - args: {}", wincmd::join(&args));
	  let mut command = Command::new("cl.exe");
		command
			.args(args.as_slice())
			.arg("/Fi".to_string() + temp_file.path().display().to_string().as_slice());
		match command.output() {
			Ok(output) => {
				println!("stderr: {}", String::from_utf8_lossy(output.error.as_slice()));
				if output.status.success() {
					match File::open(temp_file.path()).read_to_end() {
						Ok(content) => {
							match	postprocess::filter_preprocessed(content.as_slice(), &task.marker_precompiled, task.output_precompiled.is_some()) {
								Ok(output) => {
									{
										use std::hash::Writer;
										hash.write(output.as_slice());
									}
									Ok(PreprocessResult{
										hash: hash.hexdigest(),
										content: output
									})
								}
								Err(e) => Err(IoError {
									kind: IoErrorKind::InvalidInput,
									desc: "Can't parse preprocessed file",
									detail: Some(e)
								})
							}
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
			Err(e) => Err(e)
		}
	}
}
