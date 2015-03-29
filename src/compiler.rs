use std::io::{Error, ErrorKind};
use std::path::PathBuf;
use std::process::{Command, Output};

// Scope of command line argument.
#[derive(Copy)]
#[derive(Debug)]
#[derive(PartialEq)]
pub enum Scope {
	// Preprocessing argument
	Preprocessor,
	// Compiler argument
	Compiler,
	// Preprocessor & compiler argument
	Shared,
	// Unknown argument - local build only
	Ignore,
}

#[derive(Copy)]
#[derive(Debug)]
#[derive(PartialEq)]
pub enum InputKind {
	Source,
	Marker,
	Precompiled,
}

#[derive(Copy)]
#[derive(Debug)]
#[derive(PartialEq)]
pub enum OutputKind {
	Object,
	Marker,
}

#[derive(Debug)]
#[derive(PartialEq)]
pub enum Arg {
	Flag{scope:Scope, flag: String},
	Param{scope:Scope, flag: String, value: String},
	Input{kind:InputKind, flag: String, file: String},
	Output{kind:OutputKind, flag: String, file: String}
}

#[derive(Clone)]
#[derive(Debug)]
pub struct CommandInfo {
	// Program executable
	pub program: PathBuf,
	// Working directory
	pub current_dir: Option<PathBuf>,
}

impl CommandInfo {
	pub fn to_command(&self) -> Command {
		let mut command = Command::new(&self.program);
		match self.current_dir {
			Some(ref v) => {command.current_dir(&v);}
			_ => {}
		};
		command
	}
}

#[derive(Debug)]
pub struct OutputInfo {
	pub status: Option<i32>,
	pub stdout: Vec<u8>,
	pub stderr: Vec<u8>,
}

impl OutputInfo {
	pub fn new(output: Output) -> OutputInfo {
		OutputInfo {
			status: output.status.code(),
			stdout: output.stdout,
			stderr: output.stderr,
		}
	}

	pub fn success(&self) -> bool {
		match self.status {
			Some(e) if e == 0 => true,
			_ => false,
		}
	}
}

pub struct CompilationTask {
	// Original compiler executable.
	pub command: CommandInfo,
	// Parsed arguments.
	pub args: Vec<Arg>,
	// Source language.
	pub language: String,
	// Input source file name.
	pub input_source: PathBuf,
	// Input precompiled header file name.
	pub input_precompiled: Option<PathBuf>,
	// Output object file name.
	pub output_object: PathBuf,
	// Output precompiled header file name.
	pub output_precompiled: Option<PathBuf>,
	// Marker for precompiled header.
	pub marker_precompiled: Option<String>,
}

pub enum PreprocessResult {
	Success(PreprocessedSource),
	Failed(OutputInfo)
}

pub struct PreprocessedSource {
	// Hash
	pub hash: String,
	// Source file names
	pub sources: Vec<PathBuf>,
	// Preprocessed file
	pub content: Vec<u8>,
}

pub trait Compiler {
	// Parse compiler arguments.
	fn create_task(&self, command: CommandInfo, args: &[String]) -> Result<CompilationTask, String>;

	// Preprocessing source file.
	fn preprocess_step(&self, task: &CompilationTask) -> Result<PreprocessResult, Error>;

	// Compile preprocessed file.
	fn compile_step(&self, task: &CompilationTask, preprocessed: PreprocessedSource) -> Result<OutputInfo, Error>;
	
	// Run preprocess and compile.
	fn try_compile(&self, command: CommandInfo, args: &[String]) -> Result<OutputInfo, Error> {
		match self.create_task(command, args) {
			Ok(task) => {
				match try! (self.preprocess_step(&task)) {
					PreprocessResult::Success(preprocessed) => self.compile_step(&task, preprocessed),
					PreprocessResult::Failed(output) => Ok(output),
				}
			}
			Err(e) => Err(Error::new(ErrorKind::InvalidInput, "Can't parse command line arguments", Some(e)))
		}
	}

	// Run preprocess and compile.
	fn compile(&self, command: CommandInfo, args: &[String]) -> Result<OutputInfo, Error> {
		match self.try_compile(command.clone(), args) {
			Ok(output) => Ok(output),
			// todo: log error reason
			Err(e) => {
				println! ("Can't use octobuild for compiling file, use failback compilation: {:?}", e);
				command.to_command().args(args).output().map(|o| OutputInfo::new(o))
			}
		}
	}
}
