pub use super::super::compiler::*;

use super::super::filter::comments::CommentsRemover;
use super::super::io::memstream::MemStream;

use std::io;
use std::io::{Error, Read};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{channel, Receiver};
use std::thread;

pub struct ClangCompiler {
}

impl ClangCompiler {
	pub fn new() -> Self {
		ClangCompiler {
		}
	}
}

impl Compiler for ClangCompiler {
	fn create_task(&self, command: CommandInfo, args: &[String]) -> Result<Option<CompilationTask>, String> {
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
		args.push(task.input_source.display().to_string());
		args.push("-o".to_string());
		args.push("-".to_string());

		execute(task.command.to_command().args(&args))
	}

	// Compile preprocessed file.
	fn compile_prepare_step(&self, task: CompilationTask, preprocessed: MemStream) -> Result<CompileStep, Error> {
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

		Ok(CompileStep::new(task, preprocessed, args, Vec::new(), outputs))
	}

	fn compile_task(&self, task: CompileStep) -> Result<OutputInfo, Error> {
		// Run compiler.
		task.command.to_command()
		.args(&task.args)
		.arg("-".to_string())
		.arg("-o".to_string())
		.arg(task.output_object.display().to_string())
		.stdin(Stdio::piped())
		.spawn()
		.and_then(|mut child| {
			try! (task.preprocessed.copy(child.stdin.as_mut().unwrap()));
			let _ = task.preprocessed;
			child.wait_with_output()
		})
		.map(|o| OutputInfo::new(o))
	}
}

fn execute(command: &mut Command) -> Result<PreprocessResult, Error> {
	let mut child = try! (command
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn());
	drop(child.stdin.take());

	fn read_stdout<T: Read>(stream: Option<T>) -> MemStream {
		stream.map_or(Ok(MemStream::new()), |mut stream| {
			let mut ret = MemStream::new();
			io::copy(&mut stream, &mut ret).map(|_| ret)
		}).unwrap_or(MemStream::new())
	}

	fn read_stderr<T: Read + Send + 'static>(stream: Option<T>) -> Receiver<Result<Vec<u8>, Error>> {
		let (tx, rx) = channel();
		match stream {
			Some(mut stream) => {
				thread::spawn(move || {
					let mut ret = Vec::new();
					let res = stream.read_to_end(&mut ret).map(|_| ret);
					tx.send(res).unwrap();
				});
			}
			None => tx.send(Ok(Vec::new())).unwrap()
		}
		rx
	}

	fn bytes(stream: Receiver<Result<Vec<u8>, Error>>) -> Vec<u8> {
		stream.recv().unwrap().unwrap_or(Vec::new())
	}

	let stdout = read_stdout(child.stdout.take().map(|f| CommentsRemover::new(f)));
	let rx_err = read_stderr(child.stderr.take());
	let status = try!(child.wait());
	let stderr = bytes(rx_err);

	if status.success() {
		Ok(PreprocessResult::Success(stdout))
	} else {
		Ok(PreprocessResult::Failed(OutputInfo{
			status: status.code(),
			stdout: Vec::new(),
			stderr: stderr,
		}))
	}
}
