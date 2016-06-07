use std::collections::HashMap;
use std::collections::hash_map;
use std::env;
use std::iter::FromIterator;
use std::fmt::{Display, Formatter};
use std::io::{Error, ErrorKind};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::process::{Command, Output};
use std::hash::{SipHasher, Hash, Hasher};
use super::io::memstream::MemStream;
use super::io::statistic::Statistic;
use super::cache::{Cache, FileHasher};

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

#[derive(Debug)]
pub struct CommandEnv {
	map: HashMap<String, String>,
}

#[derive(Clone)]
#[derive(Debug)]
pub struct CommandInfo {
	// Program executable
	pub program: PathBuf,
	// Working directory
	pub current_dir: Option<PathBuf>,
	// Environment variables
	pub env: Arc<CommandEnv>,
}

impl CommandEnv {
	pub fn new() -> Self {
		CommandEnv {
			map: HashMap::new(),
		}
	}

	pub fn get<K: Into<String>>(&self, key: K) -> Option<&str> {
		self.map.get(&CommandEnv::normalize_key(key.into())).map(|s| s.as_str())
	}

	pub fn insert<K: Into<String>, V: Into<String>>(&mut self, key: K, value: V) -> Option<String> {
		self.map.insert(CommandEnv::normalize_key(key.into()), value.into())
	}

	pub fn iter(&self) -> hash_map::Iter<String, String> {
		self.map.iter()
	}

	#[cfg(unix)]
	fn normalize_key(key: String) -> String {
		key.into()
	}

	#[cfg(windows)]
	fn normalize_key(key: String) -> String {
		key.to_uppercase()
	}
}

impl FromIterator<(String, String)> for CommandEnv {
	fn from_iter<T: IntoIterator<Item=(String, String)>>(iter: T) -> Self {
		let mut result = CommandEnv::new();
		for (key, value) in iter {
			result.insert(key, value);
		}
		result
	}
}

impl CommandInfo {
	pub fn simple(path: &Path) -> Self {
		CommandInfo {
			program: path.to_path_buf(),
			current_dir: env::current_dir().ok(),
			env: Arc::new(env::vars().collect()),
		}
	}

	pub fn to_command(&self) -> Command {
		let mut command = Command::new(&self.program);
		command.env_clear();
		for (key, value) in self.env.iter() {
			command.env(key.clone(), value.clone());
		}
		match self.current_dir {
			Some(ref v) => {command.current_dir(&v);}
			_ => {}
		};
		command
	}

	#[cfg(unix)]
	pub fn find_executable(&self) -> Option<PathBuf> {
		self.find_executable_native(false)
	}

	#[cfg(windows)]
	pub fn find_executable(&self) -> Option<PathBuf> {
		self.find_executable_native(true)
	}

	fn find_executable_native(&self, allow_current_dir: bool) -> Option<PathBuf> {
		let executable = self.program.clone();
		// Can't execute directory
		if executable.file_name().is_none() {
			return None;
		}
		// Check absolute path
		if executable.is_absolute() {
			return fn_find_exec(executable);
		}
		// Check current catalog
		if allow_current_dir || executable.parent().map_or(false, |path| path.as_os_str().len() > 0) {
			match self.current_dir
			.as_ref()
			.map(|c| c.join(&executable))
			.and_then(|c| fn_find_exec(c)) {
				Some(exe) => { return Some(exe); }
				None => {},
			}
		}
		// Check path environment variable
		match self.env.get("PATH")
		{
			Some(paths) => {
				for path in env::split_paths(&paths) {
					match fn_find_exec(path.join(&executable)) {
						Some(exe) => { return Some(exe); }
						None => {}
					}
				}
			}
			None => {}
		}
		None
	}
}

#[derive(Debug)]
pub struct OutputInfo {
	pub status: Option<i32>,
	pub stdout: Vec<u8>,
	pub stderr: Vec<u8>,
}

impl OutputInfo {
	pub fn new(output: Output) -> Self {
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
	// Compilation toolchain.
	pub toolchain: Arc<Toolchain>,
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

pub struct CompileStep {
	// Original compiler executable.
	pub command: CommandInfo,
	// Compiler arguments.
	pub args: Vec<String>,
	// Input precompiled header file name.
	pub input_precompiled: Option<PathBuf>,
	// Output object file name.
	pub output_object: PathBuf,
	// Output precompiled header file name.
	pub output_precompiled: Option<PathBuf>,
	// Preprocessed source file.
	pub preprocessed: MemStream,
}

impl CompileStep {
	pub fn new(task: CompilationTask, preprocessed: MemStream, args: Vec<String>, use_precompiled: bool) -> Self {
		assert!(use_precompiled || task.input_precompiled.is_none());
		CompileStep {
			command: task.command,
			output_object: task.output_object,
			output_precompiled: task.output_precompiled,
			input_precompiled: match use_precompiled {
				true => task.input_precompiled,
				false => None,
			},
			args: args,
			preprocessed: preprocessed,
		}
	}
}

pub enum PreprocessResult {
	Success(MemStream),
	Failed(OutputInfo)
}

pub trait Toolchain {
	// Get toolchain identificator.
	fn identifier(&self) -> Option<String>;
	// Compile preprocessed file.
	fn compile_step(&self, task: CompileStep) -> Result<OutputInfo, Error>;
}

pub trait Compiler {
	// Resolve toolchain for command execution.
	fn resolve_toolchain(&self, command: &CommandInfo) -> Option<Arc<Toolchain>>;

	// Parse compiler arguments.
	fn create_task(&self, command: CommandInfo, args: &[String]) -> Result<Option<CompilationTask>, String>;

	// Preprocessing source file.
	fn preprocess_step(&self, task: &CompilationTask) -> Result<PreprocessResult, Error>;

	// Compile preprocessed file.
	fn compile_prepare_step(&self, task: CompilationTask, preprocessed: MemStream) -> Result<CompileStep, Error>;

	// Run preprocess and compile.
	fn try_compile(&self, command: CommandInfo, args: &[String], cache: &Cache, statistic: &RwLock<Statistic>) -> Result<Option<OutputInfo>, Error> {
		match self.create_task(command, args) {
			Ok(Some(task)) => {
				let toolchain = task.toolchain.clone();
				match try!(self.preprocess_step(&task)) {
					PreprocessResult::Success(preprocessed) => self.compile_prepare_step(task, preprocessed)
					.and_then(|task| compile_step_cached(task, cache, statistic, toolchain)),
					PreprocessResult::Failed(output) => Ok(output),
				}.map(|v| Some(v))
			}
			Ok(None) => Ok(None),
			Err(e) => Err(Error::new(ErrorKind::InvalidInput, CompilerError::InvalidArguments(e)))
		}
	}

	// Run preprocess and compile.
	fn compile(&self, command: CommandInfo, args: &[String], cache: &Cache, statistic: &RwLock<Statistic>) -> Result<OutputInfo, Error> {
		match self.try_compile(command.clone(), args, cache, statistic) {
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

pub struct ToolchainHolder {
	toolchains: Arc<RwLock<HashMap<PathBuf, Arc<Toolchain>>>>,
}

impl ToolchainHolder {
	pub fn new() -> Self {
		ToolchainHolder {
			toolchains: Arc::new(RwLock::new(HashMap::new())),
		}
	}

	pub fn resolve<F: FnOnce(PathBuf) -> Arc<Toolchain>>(&self, command: &CommandInfo, factory: F) -> Option<Arc<Toolchain>> {
		command.find_executable()
		.and_then(|path| -> Option<Arc<Toolchain>> {
			{
				let read_lock = self.toolchains.read().unwrap();
				match read_lock.get(&path) {
					Some(t) => { return Some(t.clone()); }
					None => {}
				}
			}
			{
				let mut write_lock = self.toolchains.write().unwrap();
				Some(write_lock.entry(path.clone()).or_insert_with(|| factory(path)).clone())
			}
		})
	}
}

fn compile_step_cached(task: CompileStep, cache: &Cache, statistic: &RwLock<Statistic>, toolchain: Arc<Toolchain>) -> Result<OutputInfo, Error>	{
	let mut hasher = SipHasher::new();
	// Get hash from preprocessed data
	task.preprocessed.hash(&mut hasher);
	// Hash arguments
	hasher.write_usize(task.args.len());
	for arg in task.args.iter() {
		hash_bytes(&mut hasher, &arg.as_bytes());
	}
	// Hash input files
	match task.input_precompiled {
		Some(ref path) => {
			hash_bytes(&mut hasher, try!(cache.file_hash(&path)).as_bytes());
		},
		None => {
			hasher.write_usize(0);
		}
	}
	// Store output precompiled flag
	task.output_precompiled.is_some().hash(&mut hasher);

	// Output files list
	let mut outputs: Vec<PathBuf> = Vec::new();
	outputs.push(task.output_object.clone());
	match task.output_precompiled {
		Some(ref path) => { outputs.push(path.clone()); }
		None => {}
	}

	// Try to get files from cache or run
	cache.run_file_cached(statistic, hasher.finish(), &outputs, || -> Result<OutputInfo, Error> {
		toolchain.compile_step(task)
	}, || true)
}

fn hash_bytes<H: Hasher>(hasher: &mut H, bytes: &[u8]) {
	hasher.write_usize(bytes.len());
	hasher.write(&bytes);
}

fn fn_find_exec(path: PathBuf) -> Option<PathBuf> {
	fn_find_exec_native(path).and_then(|path| path.canonicalize().ok())
}

#[cfg(windows)]
fn fn_find_exec_native(mut path: PathBuf) -> Option<PathBuf> {
	if !path.is_absolute() {
		return None
	}
	if path.is_file() {
		return Some(path.to_path_buf());
	}
	let name_with_ext = path.file_name()
	.map(|n| {
		let mut name = n.to_os_string();
		name.push(".exe");
		name
	});
	match name_with_ext {
		Some(n) => {
			path.set_file_name(n);
			if path.is_file() {
				Some(path)
			} else {
				None
			}
		},
		None => None,
	}
}

#[cfg(unix)]
fn fn_find_exec_native(path: PathBuf) -> Option<PathBuf> {
	use std::os::unix::fs::PermissionsExt;
	if !path.is_absolute() {
		return None
	}
	match path.metadata() {
		Ok(ref meta) if meta.is_file() && (meta.permissions().mode() & 0o100 == 0o100) => Some(path),
		_ => None,
	}
}
