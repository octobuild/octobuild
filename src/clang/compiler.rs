use std::ffi::OsString;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::Arc;
use std::{env, fs};

use regex::Regex;

use crate::compiler::CompileInput::{Preprocessed, Source};
use crate::compiler::{
    Arg, CommandInfo, CompilationTask, CompileStep, Compiler, CompilerOutput, OsCommandArgs,
    OutputInfo, PreprocessResult, Scope, SharedState, Toolchain, ToolchainHolder,
};
use crate::lazy::Lazy;
use lazy_static::lazy_static;
use os_str_bytes::OsStrBytes;

lazy_static! {
    static ref RE_CLANG: regex::bytes::Regex =
        regex::bytes::Regex::new(r"(?i)^(.*clang(:?\+\+)?)(-\d+\.\d+)?(?:.exe)?$").unwrap();
}

#[derive(Default)]
pub struct ClangCompiler {
    toolchains: ToolchainHolder,
}

impl ClangCompiler {
    #[must_use]
    pub fn new() -> Self {
        ClangCompiler::default()
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
            identifier: Lazy::default(),
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

fn collect_args(
    args: &[Arg],
    target_scope: Scope,
    run_second_cpp: bool,
    output_precompiled: bool,
    into: &mut Vec<OsString>,
) {
    for arg in args {
        match arg {
            Arg::Flag { scope, flag } => {
                if scope.matches(target_scope, run_second_cpp, output_precompiled) {
                    into.push(OsString::from(format!("-{flag}")));
                }
            }
            Arg::Param { scope, flag, value } => {
                if scope.matches(target_scope, run_second_cpp, output_precompiled) {
                    into.push(OsString::from(format!("-{flag}")));
                    into.push(OsString::from(value));
                }
            }
            Arg::Input { .. } | Arg::Output { .. } => {}
        };
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
    ) -> crate::Result<Vec<CompilationTask>> {
        super::prepare::create_tasks(command, args)
    }

    fn run_preprocess(
        &self,
        state: &SharedState,
        task: &CompilationTask,
    ) -> crate::Result<PreprocessResult> {
        let mut args = vec![
            OsString::from("-E"),
            OsString::from("-frewrite-includes"),
            OsString::from("-x"),
            OsString::from(&task.language),
            OsString::from(&task.input_source),
            OsString::from("-o"),
            OsString::from("-"),
        ];
        collect_args(
            &task.shared.args,
            Scope::Preprocessor,
            state.run_second_cpp,
            false,
            &mut args,
        );

        let output = state.wrap_slow(|| -> crate::Result<Output> {
            let mut command = task.shared.command.to_command();
            let response_file =
                state.do_response_file(OsCommandArgs::Regular(args), &mut command)?;
            let output = command.output()?;
            drop(response_file);

            if let Some(ref deps_file) = task.shared.deps_file {
                let data = fs::read_to_string(deps_file)?;
                if let Some(end) = data.strip_prefix('-') {
                    let mut f = File::create(deps_file)?;
                    f.write_all(&task.output_object.to_raw_bytes())?;
                    f.write_all(end.as_bytes())?;
                }
            }

            Ok(output)
        })?;

        if output.status.success() {
            Ok(PreprocessResult::Success(CompilerOutput::Vec(
                output.stdout,
            )))
        } else {
            Ok(PreprocessResult::Failed(OutputInfo {
                status: output.status.code(),
                stdout: output.stdout,
                stderr: output.stderr,
            }))
        }
    }

    // Compile preprocessed file.
    fn create_compile_step(
        &self,
        state: &SharedState,
        task: &CompilationTask,
        preprocessed: CompilerOutput,
    ) -> CompileStep {
        let mut args = vec![OsString::from("-x"), OsString::from(&task.language)];
        collect_args(
            &task.shared.args,
            Scope::Compiler,
            state.run_second_cpp,
            task.shared.pch_out.is_some(),
            &mut args,
        );

        CompileStep::new(task, preprocessed, args, false, state.run_second_cpp)
    }

    fn run_compile(&self, state: &SharedState, task: CompileStep) -> crate::Result<OutputInfo> {
        let mut args = task.args.clone();
        args.push(OsString::from("-c"));
        match &task.input {
            Preprocessed(_) => args.push(OsString::from("-")),
            Source(source) => args.push(OsString::from(&source.path)),
        };

        args.push(OsString::from("-o"));
        match task.output_object {
            None => args.push(OsString::from("-")),
            Some(v) => args.push(OsString::from(v)),
        };

        // Run compiler.
        state.wrap_slow(|| {
            // TODO: response file

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

            command
                .stdin(match &task.input {
                    Preprocessed(_) => Stdio::piped(),
                    Source(_) => Stdio::null(),
                })
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());

            let response_file =
                state.do_response_file(OsCommandArgs::Regular(args), &mut command)?;
            let mut child = command.spawn()?;

            if let Preprocessed(preprocessed) = task.input {
                preprocessed.copy(child.stdin.as_mut().unwrap())?;
            }

            let output = child.wait_with_output()?;
            drop(response_file);
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

    Some(format!("{base_name} {version} {target}"))
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
