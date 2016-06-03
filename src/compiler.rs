use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::io::{Error, ErrorKind};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::process::{Command, Output};

use super::io::memstream::MemStream;
use super::io::statistic::Statistic;

#[derive(Debug)]
pub enum CompilerError {
	InvalidArguments(String),
}
				
impl Display for CompilerError {
	fn fmt(&self, f: &mut Formatter) -> Result<(), ::std::fmt::Error> {
		match self {
			&CompilerError::InvalidArguments(ref arg) => write!(f, "can't parse command line arguments: {}", arg),
		}
	}
}

impl ::std::error::Error for CompilerError {
	fn description(&self) -> &str {
		match self {
			&CompilerError::InvalidArguments(_) => "can't parse command line arguments",
		}
	}

	fn cause(&self) -> Option<&::std::error::Error> {
		None
	}
}

// Scope of command line argument.
#[derive(Copy)]
#[derive(Clone)]
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
#[derive(Clone)]
#[derive(Debug)]
#[derive(PartialEq)]
pub enum InputKind {
	Source,
	Marker,
	Precompiled,
}

#[derive(Copy)]
#[derive(Clone)]
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
	// Environment variables
	pub env: Arc<HashMap<String, String>>,
}

impl CommandInfo {
	pub fn to_command(&self) -> Command {
		let mut command = Command::new(&self.program);
		for (key, value) in self.env.iter() {
			command.env(key.clone(), value.clone());
		}
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
	// Preprocessed file
	pub content: MemStream,
}

pub trait Compiler {
	// Parse compiler arguments.
	fn create_task(&self, command: CommandInfo, args: &[String]) -> Result<Option<CompilationTask>, String>;

	// Preprocessing source file.
	fn preprocess_step(&self, task: &CompilationTask) -> Result<PreprocessResult, Error>;

	// Compile preprocessed file.
	fn compile_step(&self, task: &CompilationTask, preprocessed: PreprocessedSource, statistic: &RwLock<Statistic>) -> Result<OutputInfo, Error>;

	// Compile preprocessed file.
	fn compile_task(&self, task: &CompilationTask, preprocessed: PreprocessedSource, args: Vec<String>) -> Result<OutputInfo, Error>;
	
	// Run preprocess and compile.
	fn try_compile(&self, command: CommandInfo, args: &[String], statistic: &RwLock<Statistic>) -> Result<Option<OutputInfo>, Error> {
		match self.create_task(command, args) {
			Ok(Some(task)) => {
				match try! (self.preprocess_step(&task)) {
					PreprocessResult::Success(preprocessed) => self.compile_step(&task, preprocessed, statistic),
					PreprocessResult::Failed(output) => Ok(output),
				}.map(|v| Some(v))
			}
			Ok(None) => Ok(None),
			Err(e) => Err(Error::new(ErrorKind::InvalidInput, CompilerError::InvalidArguments(e)))
		}
	}

	// Run preprocess and compile.
	fn compile(&self, command: CommandInfo, args: &[String], statistic: &RwLock<Statistic>) -> Result<OutputInfo, Error> {
		match self.try_compile(command.clone(), args, statistic) {
			Ok(Some(output)) => Ok(output),
			Ok(None) => {
				command.to_command().args(args).output().map(|o| OutputInfo::new(o))
			}
			// todo: log error reason
			Err(e) => {
				println! ("Can't use octobuild for compiling file, use failback compilation: {:?}", e);
				command.to_command().args(args).output().map(|o| OutputInfo::new(o))
			}
		}
	}
}
