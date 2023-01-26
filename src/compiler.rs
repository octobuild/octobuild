use std::cmp::max;
use std::collections::hash_map;
use std::collections::HashMap;
use std::env;
use std::io::{Error, ErrorKind, Write};
use std::iter::FromIterator;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use ipc::Semaphore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::cache::{Cache, FileHasher};
use crate::config::Config;
use crate::io::statistic::Statistic;

use crate::cmd;
use crate::compiler::CompileInput::{Preprocessed, Source};
use crate::io::memstream::MemStream;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CompilerError {
    #[error("can't parse command line arguments: {0}")]
    InvalidArguments(String),
    #[error("can't find toolchain for: {0}")]
    ToolchainNotFound(PathBuf),
}

// Scope of command line argument.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
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

impl Scope {
    pub fn matches(self, scope: Scope, run_second_cpp: bool, output_precompiled: bool) -> bool {
        match scope {
            Scope::Preprocessor => self == Scope::Preprocessor || self == Scope::Shared,
            Scope::Compiler => {
                if run_second_cpp {
                    self != Scope::Ignore
                } else {
                    self == Scope::Compiler
                        || self == Scope::Shared
                        || self == Scope::Preprocessor && output_precompiled
                }
            }
            Scope::Shared => self == Scope::Shared,
            Scope::Ignore => false,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum InputKind {
    Source,
    Marker,
    Precompiled,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum OutputKind {
    Object,
    Marker,
}

#[derive(Debug, Eq, PartialEq)]
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
            scope,
            flag: flag.into(),
        }
    }
    pub fn param<F: Into<String>, V: Into<String>>(scope: Scope, flag: F, value: V) -> Arg {
        Arg::Param {
            scope,
            flag: flag.into(),
            value: value.into(),
        }
    }
    pub fn input<F: Into<String>, P: Into<String>>(kind: InputKind, flag: F, file: P) -> Arg {
        Arg::Input {
            kind,
            flag: flag.into(),
            file: file.into(),
        }
    }
    pub fn output<F: Into<String>, P: Into<String>>(kind: OutputKind, flag: F, file: P) -> Arg {
        Arg::Output {
            kind,
            flag: flag.into(),
            file: file.into(),
        }
    }
}

#[derive(Debug, Default)]
pub struct CommandEnv {
    map: HashMap<String, String>,
}

#[derive(Clone, Debug)]
pub struct CommandInfo {
    // Program executable
    pub program: PathBuf,
    // Working directory
    pub current_dir: Option<PathBuf>,
    // Environment variables
    pub env: Arc<CommandEnv>,
}

pub struct SharedState {
    pub semaphore: Semaphore,
    pub cache: Cache,
    pub statistic: Statistic,
    pub run_second_cpp: bool,
}

#[derive(Default)]
pub struct CompilerGroup(Vec<Box<dyn Compiler>>);

impl SharedState {
    pub fn new(config: &Config) -> Result<Self, Error> {
        let semaphore = Semaphore::new("octobuild-worker", max(config.process_limit, 1_usize))?;
        Ok(SharedState {
            semaphore,
            statistic: Statistic::new(),
            cache: Cache::new(config),
            run_second_cpp: config.run_second_cpp,
        })
    }

    pub fn wrap_slow<T, F: FnOnce() -> T>(&self, func: F) -> T {
        let guard = self.semaphore.access();
        let result = func();
        drop(guard);
        result
    }
}

impl CommandEnv {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn get<K: Into<String>>(&self, key: K) -> Option<&str> {
        self.map
            .get(&CommandEnv::normalize_key(key.into()))
            .map(|s| s.as_str())
    }

    pub fn insert<K: Into<String>, V: Into<String>>(&mut self, key: K, value: V) -> Option<String> {
        self.map
            .insert(CommandEnv::normalize_key(key.into()), value.into())
    }

    pub fn iter(&self) -> hash_map::Iter<String, String> {
        self.map.iter()
    }

    #[cfg(unix)]
    fn normalize_key(key: String) -> String {
        key
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
        match self.current_dir.as_ref() {
            None => path.to_path_buf(),
            Some(cwd) => cwd.join(path),
        }
    }

    pub fn to_command(&self) -> Command {
        let mut command = Command::new(&self.program);
        command.env_clear();
        for (key, value) in self.env.iter() {
            command.env(key.clone(), value.clone());
        }
        if let Some(ref v) = self.current_dir {
            command.current_dir(v);
        }
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
        executable.file_name()?;
        // Check absolute path
        if executable.is_absolute() {
            return fn_find_exec(executable);
        }
        // Check current catalog
        if allow_current_dir
            || executable
                .parent()
                .map_or(false, |path| !path.as_os_str().is_empty())
        {
            if let Some(exe) = self
                .current_dir
                .as_ref()
                .map(|c| c.join(&executable))
                .and_then(fn_find_exec)
            {
                return Some(exe);
            }
        }
        // Check path environment variable
        if let Some(paths) = self.env.get("PATH") {
            for path in env::split_paths(&paths) {
                if let Some(exe) = fn_find_exec(path.join(&executable)) {
                    return Some(exe);
                }
            }
        }
        None
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct OutputInfo {
    pub status: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

pub struct BuildTaskResult {
    pub output: Result<OutputInfo, Error>,
    pub duration: Duration,
}

impl BuildTaskResult {
    pub fn print_output(&self) -> std::io::Result<()> {
        match &self.output {
            Ok(output) => {
                if !output.success() {
                    println!(
                        "ERROR: Task failed with exit code: {}",
                        output
                            .status
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| "unknown".to_string())
                    );
                }
                std::io::stdout().write_all(&output.stdout)?;
                std::io::stderr().write_all(&output.stderr)?;
            }
            Err(e) => {
                eprintln!("ERROR: {}", e);
            }
        }
        Ok(())
    }
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
        matches!(self.status, Some(e) if e == 0)
    }
}

#[derive(Debug)]
pub struct CompilationArgs {
    // Original compiler executable.
    pub command: CommandInfo,
    // Parsed arguments.
    pub args: Vec<Arg>,
    // Input precompiled header file name.
    pub input_precompiled: Option<PathBuf>,
    // Output precompiled header file name.
    pub output_precompiled: Option<PathBuf>,
    // Marker for precompiled header.
    pub marker_precompiled: Option<String>,
    pub deps_file: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct CompilationTask {
    // Compilation  arguments.
    pub shared: Arc<CompilationArgs>,
    // Source language.
    pub language: String,
    // Input source file name.
    pub input_source: PathBuf,
    // Output object file name.
    pub output_object: PathBuf,
}

pub struct SourceInput {
    pub path: PathBuf,
    pub current_dir: Option<PathBuf>,
}

pub enum CompileInput {
    Preprocessed(CompilerOutput),
    Source(SourceInput),
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
    pub input: CompileInput,
    pub marker_precompiled: Option<String>,
}

impl CompileStep {
    pub fn new(
        task: &CompilationTask,
        preprocessed: CompilerOutput,
        args: Vec<String>,
        use_precompiled: bool,
        run_second_cpp: bool,
    ) -> Self {
        assert!(use_precompiled || task.shared.input_precompiled.is_none());
        CompileStep {
            output_object: Some(task.output_object.clone()),
            output_precompiled: task.shared.output_precompiled.clone(),
            input_precompiled: if use_precompiled {
                task.shared.input_precompiled.clone()
            } else {
                None
            },
            args,
            input: if run_second_cpp {
                Source(SourceInput {
                    path: task.input_source.clone(),
                    current_dir: task.shared.command.current_dir.clone(),
                })
            } else {
                Preprocessed(preprocessed)
            },
            marker_precompiled: if run_second_cpp {
                task.shared.marker_precompiled.clone()
            } else {
                None
            },
        }
    }
}

pub enum CompilerOutput {
    MemSteam(MemStream),
    Vec(Vec<u8>),
}

impl CompilerOutput {
    pub fn copy<W: Write>(&self, writer: &mut W) -> std::io::Result<usize> {
        match &self {
            CompilerOutput::MemSteam(v) => v.copy(writer),
            CompilerOutput::Vec(v) => {
                writer.write_all(v)?;
                Ok(v.len())
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            CompilerOutput::MemSteam(v) => v.is_empty(),
            CompilerOutput::Vec(v) => v.is_empty(),
        }
    }

    pub fn len(&self) -> usize {
        match self {
            CompilerOutput::MemSteam(v) => v.len(),
            CompilerOutput::Vec(v) => v.len(),
        }
    }

    pub fn to_vec(&self) -> Vec<u8> {
        match self {
            CompilerOutput::MemSteam(v) => From::from(v),
            CompilerOutput::Vec(v) => v.clone(),
        }
    }
}

pub enum PreprocessResult {
    Success(CompilerOutput),
    Failed(OutputInfo),
}

pub trait Toolchain: Send + Sync {
    // Get toolchain identificator.
    fn identifier(&self) -> Option<String>;

    // Parse compiler arguments.
    fn create_tasks(
        &self,
        command: CommandInfo,
        args: &[String],
    ) -> Result<Vec<CompilationTask>, String>;
    // Preprocessing source file.
    fn preprocess_step(
        &self,
        state: &SharedState,
        task: &CompilationTask,
    ) -> Result<PreprocessResult, Error>;
    // Compile preprocessed file.
    fn compile_prepare_step(
        &self,
        state: &SharedState,
        task: &CompilationTask,
        preprocessed: CompilerOutput,
    ) -> Result<CompileStep, Error>;

    // Compile preprocessed file.
    fn compile_step(&self, state: &SharedState, task: CompileStep) -> Result<OutputInfo, Error>;
    // Compile preprocessed file.
    fn compile_memory(
        &self,
        state: &SharedState,
        mut task: CompileStep,
    ) -> Result<OutputInfo, Error> {
        task.output_object = None;
        self.compile_step(state, task)
    }

    fn compile_task(
        &self,
        state: &SharedState,
        task: &CompilationTask,
    ) -> Result<OutputInfo, Error> {
        let preprocessed = self.preprocess_step(state, task)?;
        match preprocessed {
            PreprocessResult::Success(preprocessed) => {
                self.compile_step_cached(state, task, preprocessed)
            }
            PreprocessResult::Failed(output) => Ok(output),
        }
    }

    fn compile_step_cached(
        &self,
        state: &SharedState,
        task: &CompilationTask,
        preprocessed: CompilerOutput,
    ) -> Result<OutputInfo, Error> {
        let mut hasher = Sha256::new();
        // Get hash from preprocessed data
        hasher.hash_u64(preprocessed.len() as u64);
        preprocessed.copy(&mut hasher)?;

        let step = self.compile_prepare_step(state, task, preprocessed)?;

        // Hash arguments
        hasher.hash_u64(step.args.len() as u64);
        for arg in step.args.iter() {
            hasher.hash_bytes(arg.as_bytes());
        }
        // Hash input files
        match step.input_precompiled {
            Some(ref path) => {
                hasher.hash_bytes(state.cache.file_hash(path)?.hash.as_bytes());
            }
            None => {
                hasher.hash_u64(0);
            }
        }
        // Store output precompiled flag
        hasher.hash_u8(u8::from(step.output_precompiled.is_some()));

        // Output files list
        let mut outputs: Vec<PathBuf> = Vec::new();
        if let Some(ref path) = step.output_object {
            outputs.push(path.clone());
        }
        if let Some(ref path) = step.output_precompiled {
            outputs.push(path.clone());
        }

        // Try to get files from cache or run
        state.cache.run_file_cached(
            &state.statistic,
            &hex::encode(hasher.finalize()),
            &outputs,
            || -> Result<OutputInfo, Error> { self.compile_step(state, step) },
            || true,
        )
    }
}

impl CompilerGroup {
    pub fn new() -> Self {
        Default::default()
    }

    #[allow(clippy::should_implement_trait)]
    pub fn add<C: 'static + Compiler>(mut self, compiler: C) -> Self {
        self.0.push(Box::new(compiler));
        self
    }
}

impl Compiler for CompilerGroup {
    // Resolve toolchain for command execution.
    fn resolve_toolchain(&self, command: &CommandInfo) -> Option<Arc<dyn Toolchain>> {
        self.0
            .iter()
            .filter_map(|c| c.resolve_toolchain(command))
            .next()
    }
    // Discover local toolchains.
    fn discover_toolchains(&self) -> Vec<Arc<dyn Toolchain>> {
        self.0
            .iter()
            .flat_map(|c| c.discover_toolchains())
            .collect()
    }
}

trait Hasher: Digest {
    fn hash_u64(&mut self, number: u64) {
        let mut n = number;
        let mut buf: [u8; 8] = [0; 8];
        for e in &mut buf {
            *e = (n & 0xFF) as u8;
            n >>= 8;
        }
        self.update(buf);
    }

    fn hash_u8(&mut self, number: u8) {
        self.update([number]);
    }

    fn hash_bytes(&mut self, bytes: &[u8]) {
        self.hash_u64(bytes.len() as u64);
        self.update(bytes);
    }
}

impl<D: Digest + ?Sized> Hasher for D {}

pub struct ToolchainCompilationTask {
    pub toolchain: Arc<dyn Toolchain>,
    pub task: CompilationTask,
}

#[derive(Debug, Clone)]
pub enum CommandArgs {
    Raw(String),
    Array(Vec<String>),
}

impl CommandArgs {
    pub fn append_to(&self, command: &mut Command) {
        match self {
            CommandArgs::Raw(v) => {
                #[cfg(windows)]
                {
                    use std::os::windows::process::CommandExt;
                    command.raw_arg(v);
                }
                #[cfg(not(windows))]
                {
                    command.args(cmd::native::parse(v));
                }
            }
            CommandArgs::Array(v) => {
                command.args(v);
            }
        }
    }
}

pub trait Compiler: Send + Sync {
    // Resolve toolchain for command execution.
    fn resolve_toolchain(&self, command: &CommandInfo) -> Option<Arc<dyn Toolchain>>;
    // Discover local toolchains.
    fn discover_toolchains(&self) -> Vec<Arc<dyn Toolchain>>;

    fn create_tasks(
        &self,
        command: CommandInfo,
        args: CommandArgs,
    ) -> Result<Vec<ToolchainCompilationTask>, Error> {
        let toolchain = self.resolve_toolchain(&command).ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidInput,
                CompilerError::ToolchainNotFound(command.program.clone()),
            )
        })?;

        let argv = match args {
            CommandArgs::Raw(v) => cmd::native::parse(&v)?,
            CommandArgs::Array(v) => v,
        };

        let tasks = toolchain
            .create_tasks(command, &argv)
            .map_err(|e| Error::new(ErrorKind::InvalidInput, CompilerError::InvalidArguments(e)))?;

        Ok(tasks
            .into_iter()
            .map(|task| ToolchainCompilationTask {
                toolchain: toolchain.clone(),
                task,
            })
            .collect())
    }
}

#[derive(Default)]
pub struct ToolchainHolder {
    toolchains: Arc<RwLock<HashMap<PathBuf, Arc<dyn Toolchain>>>>,
}

impl ToolchainHolder {
    pub fn new() -> Self {
        ToolchainHolder {
            toolchains: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn to_vec(&self) -> Vec<Arc<dyn Toolchain>> {
        let read_lock = self.toolchains.read().unwrap();
        read_lock.values().cloned().collect()
    }

    pub fn resolve<F: FnOnce(PathBuf) -> Arc<dyn Toolchain>>(
        &self,
        path: &Path,
        factory: F,
    ) -> Option<Arc<dyn Toolchain>> {
        {
            let read_lock = self.toolchains.read().unwrap();
            if let Some(t) = read_lock.get(path) {
                return Some(t.clone());
            }
        }
        {
            let mut write_lock = self.toolchains.write().unwrap();
            Some(
                write_lock
                    .entry(path.to_path_buf())
                    .or_insert_with(|| factory(path.to_path_buf()))
                    .clone(),
            )
        }
    }
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
        return Some(path);
    }
    if path
        .extension()
        .and_then(|ext| ext.to_str())
        .map_or(true, |s| s.to_lowercase() != "exe")
    {
        let name_with_ext = path.file_name().map(|n| {
            let mut name = n.to_os_string();
            name.push(".exe");
            name
        });
        if let Some(n) = name_with_ext {
            path.set_file_name(n);
            if path.is_file() {
                return Some(path);
            }
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
        Ok(ref meta) if meta.is_file() && (meta.permissions().mode() & 0o100 == 0o100) => {
            Some(path)
        }
        _ => None,
    }
}
