use std::fs::File;
use std::io;
use std::io::{Error, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::thread;
use std::{env, fs};

use regex::Regex;

use crate::compiler::CompileInput::{Preprocessed, Source};
use lazy_static::lazy_static;

pub use super::super::compiler::*;
use super::super::io::memstream::MemStream;
use super::super::lazy::Lazy;

lazy_static! {
    static ref RE_CLANG: regex::bytes::Regex =
        regex::bytes::Regex::new(r"(?i)^(.*clang(:?\+\+)?)(-\d+\.\d+)?(?:.exe)?$").unwrap();
}

#[derive(Default)]
pub struct ClangCompiler {
    toolchains: ToolchainHolder,
}

impl ClangCompiler {
    pub fn new() -> Self {
        Default::default()
    }
}

struct ClangToolchain {
    path: PathBuf,
    identifier: Lazy<Option<String>>,
}

impl ClangToolchain {
    pub fn new(path: PathBuf) -> Self {
        ClangToolchain {
            path,
            identifier: Default::default(),
        }
    }
}

impl Compiler for ClangCompiler {
    fn resolve_toolchain(&self, command: &CommandInfo) -> Option<Arc<dyn Toolchain>> {
        let file_name = command.program.file_name()?;

        if !RE_CLANG.is_match(file_name.to_string_lossy().as_bytes()) {
            return None;
        }

        let executable = command.find_executable()?;
        self.toolchains
            .resolve(&executable, |path| Arc::new(ClangToolchain::new(path)))
    }

    fn discover_toolchains(&self) -> Vec<Arc<dyn Toolchain>> {
        env::var_os("PATH")
            .map_or(Vec::new(), |paths| env::split_paths(&paths).collect())
            .iter()
            .filter(|path| path.is_absolute())
            .filter_map(|path| path.read_dir().ok())
            .flatten()
            .filter_map(|entry| entry.ok())
            .filter(|entry| RE_CLANG.is_match(entry.file_name().to_string_lossy().as_bytes()))
            .map(|entry| -> Arc<dyn Toolchain> { Arc::new(ClangToolchain::new(entry.path())) })
            .collect()
    }
}

impl Toolchain for ClangToolchain {
    fn identifier(&self) -> Option<String> {
        self.identifier.get(|| clang_identifier(&self.path))
    }

    fn create_tasks(
        &self,
        command: CommandInfo,
        args: &[String],
    ) -> Result<Vec<CompilationTask>, String> {
        super::prepare::create_tasks(command, args)
    }

    fn preprocess_step(
        &self,
        state: &SharedState,
        task: &CompilationTask,
    ) -> Result<PreprocessResult, Error> {
        let mut args: Vec<String> = vec!["-E", "-x", task.language.as_str(), "-frewrite-includes"]
            .iter()
            .map(|arg| arg.to_string())
            .collect();

        // Make parameters list for preprocessing.
        for arg in task.shared.args.iter() {
            match arg {
                Arg::Flag {
                    ref scope,
                    ref flag,
                } => {
                    if scope.matches(Scope::Preprocessor, state.run_second_cpp) {
                        args.push("-".to_string() + flag);
                    }
                }
                Arg::Param {
                    ref scope,
                    ref flag,
                    ref value,
                } => {
                    if scope.matches(Scope::Preprocessor, state.run_second_cpp) {
                        args.push("-".to_string() + flag);
                        args.push(value.clone());
                    }
                }
                Arg::Input { .. } => {}
                Arg::Output { .. } => {}
            };
        }

        // Add preprocessor parameters.
        args.push(task.input_source.display().to_string());
        args.push("-o".to_string());
        args.push("-".to_string());

        state.wrap_slow(|| {
            let result = execute(task.shared.command.to_command().args(&args))?;
            if let Some(ref deps_file) = task.shared.deps_file {
                let data = fs::read_to_string(deps_file)?;
                if let Some(end) = data.strip_prefix('-') {
                    let mut f = File::create(deps_file)?;
                    f.write_all(task.output_object.to_string_lossy().as_bytes())?;
                    f.write_all(end.as_bytes())?;
                }
            }
            Ok(result)
        })
    }

    // Compile preprocessed file.
    fn compile_prepare_step(
        &self,
        state: &SharedState,
        task: &CompilationTask,
        preprocessed: MemStream,
    ) -> Result<CompileStep, Error> {
        let mut args = vec!["-x".to_string(), task.language.clone()];
        for arg in task.shared.args.iter() {
            match arg {
                Arg::Flag {
                    ref scope,
                    ref flag,
                } => {
                    if scope.matches(Scope::Compiler, state.run_second_cpp) {
                        args.push("-".to_string() + flag);
                    }
                }
                Arg::Param {
                    ref scope,
                    ref flag,
                    ref value,
                } => {
                    if scope.matches(Scope::Compiler, state.run_second_cpp) {
                        args.push("-".to_string() + flag);
                        args.push(value.clone());
                    }
                }
                Arg::Input { .. } => {}
                Arg::Output { .. } => {}
            };
        }
        Ok(CompileStep::new(
            task,
            preprocessed,
            args,
            false,
            state.run_second_cpp,
        ))
    }

    fn compile_step(&self, state: &SharedState, task: CompileStep) -> Result<OutputInfo, Error> {
        // Run compiler.
        state.wrap_slow(|| {
            let mut command = Command::new(&self.path);

            match &task.input {
                Preprocessed(_) => {
                    command.env_clear();
                }
                Source(source) => {
                    if let Some(dir) = &source.current_dir {
                        command.current_dir(dir);
                    }
                }
            }

            command.arg("-c").args(&task.args);

            match &task.input {
                Preprocessed(_) => command.arg("-"),
                Source(source) => command.arg(&source.path),
            };

            command
                .arg("-o")
                .arg(
                    &task
                        .output_object
                        .map_or("-".to_string(), |path| path.display().to_string()),
                )
                .stdin(match &task.input {
                    Preprocessed(_) => Stdio::piped(),
                    Source(_) => Stdio::null(),
                })
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());

            let mut child = command.spawn()?;

            if let Preprocessed(preprocessed) = task.input {
                preprocessed.copy(child.stdin.as_mut().unwrap())?;
            }

            let output = child.wait_with_output()?;
            Ok(OutputInfo::new(output))
        })
    }
}

fn clang_parse_version(base_name: &str, stdout: &str) -> Option<String> {
    lazy_static! {
        static ref RE: Regex = Regex::new(r"^.*clang.*?\((\S+)\).*\nTarget:\s*(\S+)").unwrap();
    }

    let cap: regex::Captures = RE.captures_iter(stdout).next()?;
    let version = cap.get(1)?.as_str();
    let target = cap.get(2)?.as_str();

    Some(format!("{} {} {}", base_name, version, target))
}

fn clang_identifier(clang: &Path) -> Option<String> {
    let filename = clang.file_name()?.to_string_lossy();
    let cap: regex::bytes::Captures = RE_CLANG.captures_iter(filename.as_bytes()).next()?;
    let base_name = String::from_utf8_lossy(cap.get(1)?.as_bytes()).into_owned();
    let output = Command::new(clang.as_os_str())
        .arg("--version")
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    clang_parse_version(&base_name, &String::from_utf8_lossy(&output.stdout))
}

fn execute(command: &mut Command) -> Result<PreprocessResult, Error> {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    drop(child.stdin.take());

    fn read_stdout<T: Read>(stream: Option<T>) -> MemStream {
        stream
            .map_or(Ok(MemStream::new()), |mut stream| {
                let mut ret = MemStream::new();
                io::copy(&mut stream, &mut ret).map(|_| ret)
            })
            .unwrap_or_else(|_| MemStream::new())
    }

    fn read_stderr<T: Read + Send + 'static>(
        stream: Option<T>,
    ) -> Receiver<Result<Vec<u8>, Error>> {
        let (tx, rx) = channel();
        match stream {
            Some(mut stream) => {
                thread::spawn(move || {
                    let mut ret = Vec::new();
                    let res = stream.read_to_end(&mut ret).map(|_| ret);
                    tx.send(res).unwrap();
                });
            }
            None => tx.send(Ok(Vec::new())).unwrap(),
        }
        rx
    }

    fn bytes(stream: Receiver<Result<Vec<u8>, Error>>) -> Vec<u8> {
        stream.recv().unwrap().unwrap_or_default()
    }

    let rx_err = read_stderr(child.stderr.take());
    let stdout = read_stdout(child.stdout.take());
    let status = child.wait()?;
    let stderr = bytes(rx_err);

    if status.success() {
        Ok(PreprocessResult::Success(stdout))
    } else {
        Ok(PreprocessResult::Failed(OutputInfo {
            status: status.code(),
            stdout: Vec::new(),
            stderr,
        }))
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn test_ubuntu_14_04_clang_3_5() {
        assert_eq!(
            super::clang_parse_version(
                "prefix",
                r#"Ubuntu clang version 3.5.0-4ubuntu2~trusty2 (tags/RELEASE_350/final) (based on LLVM 3.5.0)
Target: x86_64-pc-linux-gnu
Thread model: posix
"#,
            ),
            Some("prefix tags/RELEASE_350/final x86_64-pc-linux-gnu".to_string())
        )
    }

    #[test]
    fn test_ubuntu_14_04_clang_3_6() {
        assert_eq!(
            super::clang_parse_version(
                "prefix",
                r#"Ubuntu clang version 3.6.0-2ubuntu1~trusty1 (tags/RELEASE_360/final) (based on LLVM 3.6.0)
Target: x86_64-pc-linux-gnu
Thread model: posix
"#,
            ),
            Some("prefix tags/RELEASE_360/final x86_64-pc-linux-gnu".to_string())
        )
    }

    #[test]
    fn test_ubuntu_16_04_clang_3_5() {
        assert_eq!(
            super::clang_parse_version(
                "prefix",
                r#"Ubuntu clang version 3.5.2-3ubuntu1 (tags/RELEASE_352/final) (based on LLVM 3.5.2)
Target: x86_64-pc-linux-gnu
Thread model: posix
"#,
            ),
            Some("prefix tags/RELEASE_352/final x86_64-pc-linux-gnu".to_string())
        )
    }

    #[test]
    fn test_ubuntu_16_04_clang_3_6() {
        assert_eq!(
            super::clang_parse_version(
                "prefix",
                r#"Ubuntu clang version 3.6.2-3ubuntu2 (tags/RELEASE_362/final) (based on LLVM 3.6.2)
Target: x86_64-pc-linux-gnu
Thread model: posix
"#,
            ),
            Some("prefix tags/RELEASE_362/final x86_64-pc-linux-gnu".to_string())
        )
    }

    #[test]
    fn test_ubuntu_16_04_clang_3_7() {
        assert_eq!(
            super::clang_parse_version(
                "prefix",
                r#"Ubuntu clang version 3.7.1-2ubuntu2 (tags/RELEASE_371/final) (based on LLVM 3.7.1)
Target: x86_64-pc-linux-gnu
Thread model: posix
"#,
            ),
            Some("prefix tags/RELEASE_371/final x86_64-pc-linux-gnu".to_string())
        )
    }

    #[test]
    fn test_ubuntu_16_04_clang_3_8() {
        assert_eq!(
            super::clang_parse_version(
                "prefix",
                r#"clang version 3.8.0-2ubuntu3 (tags/RELEASE_380/final)
Target: x86_64-pc-linux-gnu
Thread model: posix
InstalledDir: /usr/bin
"#,
            ),
            Some("prefix tags/RELEASE_380/final x86_64-pc-linux-gnu".to_string())
        )
    }
}
