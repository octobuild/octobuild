use capnp;

use std::collections::HashMap;
use std::collections::hash_map;
use std::env;
use std::iter::FromIterator;
use std::fmt::{Display, Formatter};
use std::io::{Error, ErrorKind};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::process::{Command, Output};
use std::hash::{Hash, Hasher, SipHasher};

use ::io::memstream::MemStream;
use ::io::statistic::Statistic;
use ::cache::{Cache, FileHasher};
use ::builder_capnp::output_info;

#[derive(Debug)]
pub enum CompilerError {
    InvalidArguments(String),
    ToolchainNotFound(PathBuf),
}

impl Display for CompilerError {
    fn fmt(&self, f: &mut Formatter) -> Result<(), ::std::fmt::Error> {
        match self {
            &CompilerError::InvalidArguments(ref arg) => write!(f, "can't parse command line arguments: {}", arg),
            &CompilerError::ToolchainNotFound(ref arg) => write!(f, "can't find toolchain for: {}", arg.display()),
        }
    }
}

impl ::std::error::Error for CompilerError {
    fn description(&self) -> &str {
        match self {
            &CompilerError::InvalidArguments(_) => "can't parse command line arguments",
            &CompilerError::ToolchainNotFound(_) => "can't find toolchain",
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
    Flag {
        scope: Scope,
        flag: String,
    },
    Param {
        scope: Scope,
        flag: String,
        value: String,
    },
    Input {
        kind: InputKind,
        flag: String,
        file: String,
    },
    Output {
        kind: OutputKind,
        flag: String,
        file: String,
    },
}

impl Arg {
    pub fn flag<F: Into<String>>(scope: Scope, flag: F) -> Arg {
        Arg::Flag {
            scope: scope,
            flag: flag.into(),
        }
    }
    pub fn param<F: Into<String>, V: Into<String>>(scope: Scope, flag: F, value: V) -> Arg {
        Arg::Param {
            scope: scope,
            flag: flag.into(),
            value: value.into(),
        }
    }
    pub fn input<F: Into<String>, P: Into<String>>(kind: InputKind, flag: F, file: P) -> Arg {
        Arg::Input {
            kind: kind,
            flag: flag.into(),
            file: file.into(),
        }
    }
    pub fn output<F: Into<String>, P: Into<String>>(kind: OutputKind, flag: F, file: P) -> Arg {
        Arg::Output {
            kind: kind,
            flag: flag.into(),
            file: file.into(),
        }
    }
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

pub struct CompilerGroup {
    compilers: Vec<Box<Compiler>>,
}

impl CommandEnv {
    pub fn new() -> Self {
        CommandEnv { map: HashMap::new() }
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
    fn from_iter<T: IntoIterator<Item = (String, String)>>(iter: T) -> Self {
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

    pub fn current_dir_join(&self, path: &Path) -> PathBuf {
        match &self.current_dir {
            &Some(ref cwd) => cwd.join(path),
            &None => path.to_path_buf(),
        }
    }

    pub fn to_command(&self) -> Command {
        let mut command = Command::new(&self.program);
        command.env_clear();
        for (key, value) in self.env.iter() {
            command.env(key.clone(), value.clone());
        }
        match self.current_dir {
            Some(ref v) => {
                command.current_dir(&v);
            }
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
                Some(exe) => {
                    return Some(exe);
                }
                None => {}
            }
        }
        // Check path environment variable
        match self.env.get("PATH") {
            Some(paths) => {
                for path in env::split_paths(&paths) {
                    match fn_find_exec(path.join(&executable)) {
                        Some(exe) => {
                            return Some(exe);
                        }
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

    pub fn read(reader: output_info::Reader) -> Result<(Self, Vec<u8>), capnp::Error> {
        let content = try!(reader.get_content()).to_vec();
        let output = OutputInfo {
            status: match reader.get_undefined() {
                false => Some(reader.get_status()),
                true => None,
            },
            stdout: try!(reader.get_stdout()).to_vec(),
            stderr: try!(reader.get_stderr()).to_vec(),
        };
        Ok((output, content))
    }

    pub fn write(&self, mut builder: output_info::Builder, content: &[u8]) {
        match self.status {
            Some(v) => {
                builder.set_undefined(false);
                builder.set_status(v);
            }
            None => {
                builder.set_undefined(true);
            }
        }
        builder.set_stdout(&self.stdout);
        builder.set_stderr(&self.stderr);
        builder.set_content(content);
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

pub struct CompileStep {
    // Compiler arguments.
    pub args: Vec<String>,
    // Input precompiled header file name.
    pub input_precompiled: Option<PathBuf>,
    // Output object file name (None - compile to stdout).
    pub output_object: Option<PathBuf>,
    // Output precompiled header file name.
    pub output_precompiled: Option<PathBuf>,
    // Preprocessed source file.
    pub preprocessed: MemStream,
}

impl CompileStep {
    pub fn new(task: CompilationTask, preprocessed: MemStream, args: Vec<String>, use_precompiled: bool) -> Self {
        assert!(use_precompiled || task.input_precompiled.is_none());
        CompileStep {
            output_object: Some(task.output_object),
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
    Failed(OutputInfo),
}

pub trait Toolchain: Send + Sync {
    // Get toolchain identificator.
    fn identifier(&self) -> Option<String>;

    // Parse compiler arguments.
    fn create_task(&self, command: CommandInfo, args: &[String]) -> Result<Option<CompilationTask>, String>;
    // Preprocessing source file.
    fn preprocess_step(&self, task: &CompilationTask) -> Result<PreprocessResult, Error>;
    // Compile preprocessed file.
    fn compile_prepare_step(&self, task: CompilationTask, preprocessed: MemStream) -> Result<CompileStep, Error>;

    // Compile preprocessed file.
    fn compile_step(&self, task: CompileStep) -> Result<OutputInfo, Error>;
    // Compile preprocessed file.
    fn compile_memory(&self, mut task: CompileStep) -> Result<(OutputInfo, Vec<u8>), Error> {
        task.output_object = None;
        self.compile_step(task)
            .map(|output| {
                (OutputInfo {
                    status: output.status,
                    stderr: output.stderr,
                    stdout: Vec::new(),
                },
                 output.stdout)
            })
    }
}

impl CompilerGroup {
    pub fn new(compilers: Vec<Box<Compiler>>) -> Self {
        CompilerGroup { compilers: compilers }
    }
}

impl Compiler for CompilerGroup {
    // Resolve toolchain for command execution.
    fn resolve_toolchain(&self, command: &CommandInfo) -> Option<Arc<Toolchain>> {
        self.compilers.iter().filter_map(|c| c.resolve_toolchain(command)).next()
    }
    // Discovery local toolchains.
    fn discovery_toolchains(&self) -> Vec<Arc<Toolchain>> {
        self.compilers.iter().flat_map(|c| c.discovery_toolchains()).collect()
    }
}

pub trait Compiler: Send + Sync {
    // Resolve toolchain for command execution.
    fn resolve_toolchain(&self, command: &CommandInfo) -> Option<Arc<Toolchain>>;

    // Discovery local toolchains.
    fn discovery_toolchains(&self) -> Vec<Arc<Toolchain>>;

    // Run preprocess and compile.
    fn try_compile(&self,
                   command: CommandInfo,
                   args: &[String],
                   cache: &Cache,
                   statistic: &Arc<Statistic>)
                   -> Result<Option<OutputInfo>, Error> {
        let toolchain = try!(self.resolve_toolchain(&command)
            .ok_or(Error::new(ErrorKind::InvalidInput,
                              CompilerError::ToolchainNotFound(command.program.clone()))));
        match toolchain.create_task(command, args) {
            Ok(Some(task)) => {
                match try!(toolchain.preprocess_step(&task)) {
                        PreprocessResult::Success(preprocessed) => {
                            toolchain.compile_prepare_step(task, preprocessed)
                                .and_then(|task| compile_step_cached(task, cache, statistic, toolchain))
                        }
                        PreprocessResult::Failed(output) => Ok(output),
                    }
                    .map(|v| Some(v))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(Error::new(ErrorKind::InvalidInput, CompilerError::InvalidArguments(e))),
        }
    }

    // Run preprocess and compile.
    fn compile(&self,
               command: CommandInfo,
               args: &[String],
               cache: &Cache,
               statistic: &Arc<Statistic>)
               -> Result<OutputInfo, Error> {
        match self.try_compile(command.clone(), args, cache, statistic) {
            Ok(Some(output)) => Ok(output),
            Ok(None) => command.to_command().args(args).output().map(|o| OutputInfo::new(o)),
            // todo: log error reason
            Err(e) => {
                info!("Can't use octobuild for compiling file, use fallback compilation: {}",
                      e);
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
        ToolchainHolder { toolchains: Arc::new(RwLock::new(HashMap::new())) }
    }

    pub fn to_vec(&self) -> Vec<Arc<Toolchain>> {
        let read_lock = self.toolchains.read().unwrap();
        read_lock.values().map(|v| v.clone()).collect()
    }

    pub fn resolve<F: FnOnce(PathBuf) -> Arc<Toolchain>>(&self, path: &Path, factory: F) -> Option<Arc<Toolchain>> {
        {
            let read_lock = self.toolchains.read().unwrap();
            match read_lock.get(path) {
                Some(t) => {
                    return Some(t.clone());
                }
                None => {}
            }
        }
        {
            let mut write_lock = self.toolchains.write().unwrap();
            Some(write_lock.entry(path.to_path_buf())
                .or_insert_with(|| factory(path.to_path_buf()))
                .clone())
        }
    }
}

fn compile_step_cached(task: CompileStep,
                       cache: &Cache,
                       statistic: &Arc<Statistic>,
                       toolchain: Arc<Toolchain>)
                       -> Result<OutputInfo, Error> {
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
        }
        None => {
            hasher.write_usize(0);
        }
    }
    // Store output precompiled flag
    task.output_precompiled.is_some().hash(&mut hasher);

    // Output files list
    let mut outputs: Vec<PathBuf> = Vec::new();
    match task.output_object {
        Some(ref path) => {
            outputs.push(path.clone());
        }
        None => {}
    }
    match task.output_precompiled {
        Some(ref path) => {
            outputs.push(path.clone());
        }
        None => {}
    }

    // Try to get files from cache or run
    cache.run_file_cached(statistic,
                          hasher.finish(),
                          &outputs,
                          || -> Result<OutputInfo, Error> { toolchain.compile_step(task) },
                          || true)
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
        return None;
    }
    if path.is_file() {
        return Some(path.to_path_buf());
    }
    if path.extension().and_then(|ext| ext.to_str()).map_or(true, |s| s.to_lowercase() != "exe") {
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
                    return Some(path);
                }
            }
            None => {}
        }
    }
    None
}

#[cfg(unix)]
fn fn_find_exec_native(path: PathBuf) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    if !path.is_absolute() {
        return None;
    }
    match path.metadata() {
        Ok(ref meta) if meta.is_file() && (meta.permissions().mode() & 0o100 == 0o100) => Some(path),
        _ => None,
    }
}
