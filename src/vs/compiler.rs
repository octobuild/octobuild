use crate::compiler::CompileInput::{Preprocessed, Source};
use crate::compiler::{
    Arg, CommandInfo, CompilationTask, CompileStep, Compiler, CompilerOutput, OutputInfo,
    PreprocessResult, Scope, SharedState, Toolchain, ToolchainHolder,
};
use crate::io::memstream::MemStream;
use crate::lazy::Lazy;
use crate::utils::OsStrExt;
use crate::vs::postprocess;
use lazy_static::lazy_static;
use regex::bytes::{NoExpand, Regex};
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::{Cursor, Error};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Arc;
use std::{env, fs};

#[derive(Default)]
pub struct VsCompiler {
    toolchains: ToolchainHolder,
}

impl VsCompiler {
    #[must_use]
    pub fn new() -> Self {
        VsCompiler::default()
    }
}

struct VsToolchain {
    path: PathBuf,
    identifier: Lazy<Option<String>>,
}

impl VsToolchain {
    pub fn new(path: PathBuf) -> Self {
        VsToolchain {
            path,
            identifier: Lazy::default(),
        }
    }
}

impl Compiler for VsCompiler {
    fn resolve_toolchain(&self, command: &CommandInfo) -> Option<Arc<dyn Toolchain>> {
        let filename_lowercase = command.program.file_name()?.to_str()?.to_lowercase();
        if filename_lowercase != "cl.exe" && filename_lowercase != "cl" {
            return None;
        }
        let executable = command.find_executable()?;
        self.toolchains
            .resolve(&executable, |path| Arc::new(VsToolchain::new(path)))
    }

    #[cfg(unix)]
    fn discover_toolchains(&self) -> Vec<Arc<dyn Toolchain>> {
        Vec::new()
    }

    #[cfg(windows)]
    fn discover_toolchains(&self) -> Vec<Arc<dyn Toolchain>> {
        use winreg::enums::*;
        use winreg::RegKey;

        lazy_static! {
            static ref RE: regex::Regex = regex::Regex::new(r"^\d+\.\d+$").unwrap();
        }

        const CL_BIN: &[&str] = &[
            "bin/cl.exe",
            "bin/x86_arm/cl.exe",
            "bin/x86_amd64/cl.exe",
            "bin/amd64_x86/cl.exe",
            "bin/amd64_arm/cl.exe",
            "bin/amd64/cl.exe",
        ];
        const VC_REG: &[&str] = &[
            "SOFTWARE\\Wow6432Node\\Microsoft\\VisualStudio\\SxS\\VC7",
            "SOFTWARE\\Microsoft\\VisualStudio\\SxS\\VC7",
        ];

        VC_REG
            .iter()
            .filter_map(|reg_path| {
                RegKey::predef(HKEY_LOCAL_MACHINE)
                    .open_subkey_with_flags(reg_path, KEY_READ)
                    .ok()
            })
            .flat_map(|key| -> Vec<String> {
                key.enum_values()
                    .filter_map(|x| x.ok())
                    .map(|(name, _)| name)
                    .filter(|name| RE.is_match(name))
                    .filter_map(|name: String| -> Option<String> { key.get_value(name).ok() })
                    .collect()
            })
            .map(|path| PathBuf::from(&path))
            .map(|path| -> Vec<PathBuf> { CL_BIN.iter().map(|bin| path.join(bin)).collect() })
            .flat_map(|paths| paths.into_iter())
            .filter(|cl| cl.exists())
            .map(|cl| -> Arc<dyn Toolchain> { Arc::new(VsToolchain::new(cl)) })
            .filter(|toolchain| toolchain.identifier().is_some())
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
                    into.push(OsString::from("/".to_string() + flag));
                }
            }
            Arg::Param { scope, flag, value } => {
                if scope.matches(target_scope, run_second_cpp, output_precompiled) {
                    into.push(OsString::from("/".to_string() + flag + value));
                }
            }
            Arg::Input { .. } | Arg::Output { .. } => {}
        };
    }
}

impl Toolchain for VsToolchain {
    fn identifier(&self) -> Option<String> {
        self.identifier.get(|| vs_identifier(&self.path))
    }

    fn create_tasks(
        &self,
        command: CommandInfo,
        args: &[String],
    ) -> Result<Vec<CompilationTask>, String> {
        super::prepare::create_tasks(command, args)
    }

    fn run_preprocess(
        &self,
        state: &SharedState,
        task: &CompilationTask,
    ) -> Result<PreprocessResult, Error> {
        let mut args = vec![
            OsString::from("/nologo"),
            OsString::from("/T".to_string() + &task.language),
            OsString::from("/E"),
            OsString::from("/we4002"), // C4002: too many actual parameters for macro 'identifier'
            OsString::from("/Fo").concat(task.output_object.as_os_str()), // /Fo option also set output path for #import directive
            OsString::from(&task.input_source),
        ];
        collect_args(
            &task.shared.args,
            Scope::Preprocessor,
            state.run_second_cpp,
            false,
            &mut args,
        );

        let output = state.wrap_slow(|| -> std::io::Result<Output> {
            let mut command = task.shared.command.to_command();
            let response_file = state.do_response_file(args, &mut command)?;
            let output = command.output()?;
            drop(response_file);
            Ok(output)
        })?;

        if output.status.success() {
            let mut content = MemStream::new();
            if task.shared.pch_in.is_some() || task.shared.pch_out.is_some() {
                postprocess::filter_preprocessed(
                    &mut Cursor::new(output.stdout),
                    &mut content,
                    &task.shared.pch_marker,
                    task.shared.pch_out.is_some(),
                )?;
                Ok(PreprocessResult::Success(CompilerOutput::MemSteam(content)))
            } else {
                Ok(PreprocessResult::Success(CompilerOutput::Vec(
                    output.stdout,
                )))
            }
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
        let mut args = vec![
            OsString::from("/nologo"),
            OsString::from("/T".to_string() + &task.language),
        ];
        if task.shared.pch_out.is_some() {
            args.push(OsString::from("/Yc"));
        }
        collect_args(
            &task.shared.args,
            Scope::Compiler,
            state.run_second_cpp,
            task.shared.pch_out.is_some(),
            &mut args,
        );
        CompileStep::new(task, preprocessed, args, true, state.run_second_cpp)
    }

    fn run_compile(&self, state: &SharedState, task: CompileStep) -> Result<OutputInfo, Error> {
        let (output_path, temp_output) = match task.output_object {
            Some(v) => (v, None),
            None => {
                let output_temp = tempfile::Builder::new()
                    .suffix(".o")
                    .tempfile_in(state.temp_dir.path())?;
                (output_temp.path().to_path_buf(), Some(output_temp))
            }
        };

        let mut args = task.args.clone();
        args.push(OsString::from("/c"));
        args.push(OsString::from("/Fo").concat(output_path.as_os_str()));

        // Output files.
        if let Some(path) = task.pch_out {
            assert!(path.is_absolute());
            args.push(OsString::from("/Fp").concat(path.as_os_str()));
        }

        let (input_path, temp_input, current_dir_override) = match &task.input {
            Preprocessed(preprocessed) => {
                let input_temp = tempfile::Builder::new()
                    .suffix(".i")
                    .tempfile_in(state.temp_dir.path())?;
                preprocessed.copy(&mut File::create(input_temp.path())?)?;
                (input_temp.path().to_path_buf(), Some(input_temp), None)
            }
            Source(source) => {
                if let Some(dir) = &source.current_dir {
                    (source.path.clone(), None, Some(dir.as_path()))
                } else {
                    (source.path.clone(), None, None)
                }
            }
        };
        args.push(OsString::from(&input_path));

        // Use precompiled header
        if let Some(path) = task.pch_in {
            assert!(path.is_absolute());
            if let Some(pch_marker) = &task.pch_marker {
                args.push(OsString::from("/Yu".to_string() + pch_marker));
            } else {
                args.push(OsString::from("/Yu"));
            }

            args.push(OsString::from("/Fp").concat(path.as_os_str()));
        }

        // Run compiler.

        let input_marker = input_path
            // Save input file name for output filter.
            .file_name()
            .and_then(OsStr::to_str)
            .map(str::as_bytes)
            .unwrap_or(b"");

        // Execute.
        let output = state.wrap_slow(|| -> std::io::Result<Output> {
            let mut command = Command::new(&self.path);

            command
                .env_clear()
                .current_dir(current_dir_override.unwrap_or_else(|| state.temp_dir.path()));

            // Copy required environment variables.
            // todo: #15 Need to make correct PATH variable for cl.exe manually
            for (name, value) in vec!["SystemDrive", "SystemRoot", "TEMP", "TMP", "PATH"]
                .iter()
                .filter_map(|name| env::var(name).ok().map(|value| (name, value)))
            {
                command.env(name, value);
            }

            let response_file = state.do_response_file(args, &mut command)?;
            let output = command.output()?;
            drop(temp_input);
            drop(response_file);
            Ok(output)
        })?;

        let content = match temp_output {
            Some(v) => fs::read(v.path())?,
            None => output.stdout,
        };

        Ok(OutputInfo {
            status: output.status.code(),
            stdout: prepare_output(input_marker, content, output.status.success()),
            stderr: output.stderr,
        })
    }
}

#[cfg(unix)]
fn vs_identifier(_: &Path) -> Option<String> {
    None
}

#[cfg(windows)]
#[allow(clippy::uninit_vec)]
fn vs_identifier(path: &Path) -> Option<String> {
    use winapi::ctypes::c_void;
    use winapi::shared::minwindef::{DWORD, LPCVOID, LPVOID, WORD};
    use winapi::um::winver;

    use log::warn;

    use std::os::windows::ffi::OsStrExt;
    use std::ptr;
    use std::slice;

    #[repr(C)]
    #[allow(clippy::upper_case_acronyms)]
    struct LANGANDCODEPAGE {
        language: WORD,
        codepage: WORD,
    }

    fn utf16<'a, T: Into<&'a OsStr>>(value: T) -> Vec<u16> {
        value
            .into()
            .encode_wide()
            .chain(Some(0).into_iter())
            .collect()
    }

    let path_raw = utf16(path.as_os_str());
    // Get version info size
    let size = unsafe { winver::GetFileVersionInfoSizeW(path_raw.as_ptr(), ptr::null_mut()) };
    if size == 0 {
        return None;
    }
    // Load version info
    let mut data: Vec<u8> = Vec::with_capacity(size as usize);
    unsafe {
        data.set_len(size as usize);
        if winver::GetFileVersionInfoW(path_raw.as_ptr(), 0, size, data.as_mut_ptr() as *mut c_void)
            == 0
        {
            return None;
        }
    }
    // Read translation
    let translation_key = unsafe {
        let mut value_size: DWORD = 0;
        let mut value_data: LPVOID = ptr::null_mut();
        if winver::VerQueryValueW(
            data.as_ptr() as LPCVOID,
            utf16(OsStr::new("\\VarFileInfo\\Translation")).as_ptr(),
            &mut value_data,
            &mut value_size,
        ) == 0
        {
            return None;
        }
        let codepage = value_data as *const LANGANDCODEPAGE;
        format!(
            "\\StringFileInfo\\{:04X}{:04X}",
            (*codepage).language,
            (*codepage).codepage
        )
    };
    // Read product version
    let product_version = unsafe {
        let mut value_size: DWORD = 0;
        let mut value_data: LPVOID = ptr::null_mut();
        if winver::VerQueryValueW(
            data.as_ptr() as LPCVOID,
            utf16(OsStr::new(&(translation_key + "\\ProductVersion"))).as_ptr(),
            &mut value_data,
            &mut value_size,
        ) == 0
        {
            return None;
        }
        if value_size == 0 {
            return None;
        }
        String::from_utf16_lossy(slice::from_raw_parts(
            value_data as *mut u16,
            (value_size - 1) as usize,
        ))
    };
    let executable_id = match read_executable_id(path) {
        Ok(id) => id,
        Err(e) => {
            warn!("{}", e);
            return None;
        }
    };
    Some(format!("cl {} {}", &product_version, executable_id))
}

#[cfg(windows)]
fn read_executable_id(path: &Path) -> Result<String, Error> {
    use byteorder::{LittleEndian, ReadBytesExt};
    use std::io::{ErrorKind, Read, Seek, SeekFrom};

    let mut header: Vec<u8> = Vec::with_capacity(0x54);

    let mut file = File::open(path)?;
    // Read MZ header
    header.resize(0x40, 0);
    file.read_exact(&mut header[..])?;
    // Check MZ header signature
    if header[0..2] != [0x4D, 0x5A] {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Unexpected file type (MZ header signature not found)",
        ));
    }
    // Read PE header offset
    let pe_offset = u64::from(Cursor::new(&header[0x3C..0x40]).read_u32::<LittleEndian>()?);
    // Read PE header
    file.seek(SeekFrom::Start(pe_offset))?;
    header.resize(0x54, 0);
    file.read_exact(&mut header[..])?;
    // Check PE header signature
    if header[0..4] != [0x50, 0x45, 0x00, 0x00] {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Unexpected file type (PE header signature not found)",
        ));
    }
    let pe_time_date_stamp = Cursor::new(&header[0x08..0x0C]).read_u32::<LittleEndian>()?;
    let pe_size_of_image = Cursor::new(&header[0x50..0x54]).read_u32::<LittleEndian>()?;
    // Read PE header information
    Ok(format!("{pe_time_date_stamp:X}{pe_size_of_image:x}"))
}

fn prepare_output(line: &[u8], mut buffer: Vec<u8>, success: bool) -> Vec<u8> {
    // Remove strage file name from output
    let mut begin =
        if (line.len() < buffer.len()) && buffer.starts_with(line) && is_eol(buffer[line.len()]) {
            line.len()
        } else {
            0
        };
    while begin < buffer.len() && is_eol(buffer[begin]) {
        begin += 1;
    }
    buffer = buffer.split_off(begin);
    if success {
        // Remove some redundant lines
        lazy_static! {
            static ref RE: Regex =
                Regex::new(r"(?m)^\S+[^:]*\(\d+\) : warning C4628: .*$\n?").unwrap();
        }
        buffer = RE.replace_all(&buffer, NoExpand(b"")).to_vec();
    }
    buffer
}

fn is_eol(c: u8) -> bool {
    matches!(c, b'\r' | b'\n')
}

#[cfg(test)]
mod test {
    use std::io::Write;

    fn check_prepare_output(original: &str, expected: &str, line: &str, success: bool) {
        let mut stream: Vec<u8> = Vec::new();
        stream.write_all(original.as_bytes()).unwrap();

        let result = super::prepare_output(line.as_bytes(), stream, success);
        assert_eq!(String::from_utf8_lossy(&result), expected);
    }

    #[test]
    fn test_prepare_output_simple() {
        check_prepare_output(
            r#"BLABLABLA
foo.c : warning C4411: foo bar
"#,
            r#"foo.c : warning C4411: foo bar
"#,
            "BLABLABLA",
            true,
        );
    }

    #[test]
    fn test_prepare_output_c4628_remove() {
        check_prepare_output(
            r#"BLABLABLA
foo.c(41) : warning C4411: foo bar
foo.c(42) : warning C4628: foo bar
foo.c(43) : warning C4433: foo bar
"#,
            r#"foo.c(41) : warning C4411: foo bar
foo.c(43) : warning C4433: foo bar
"#,
            "BLABLABLA",
            true,
        );
    }

    #[test]
    fn test_prepare_output_c4628_keep() {
        check_prepare_output(
            r#"BLABLABLA
foo.c(41) : warning C4411: foo bar
foo.c(42) : warning C4628: foo bar
foo.c(43) : warning C4433: foo bar
"#,
            r#"foo.c(41) : warning C4411: foo bar
foo.c(42) : warning C4628: foo bar
foo.c(43) : warning C4433: foo bar
"#,
            "BLABLABLA",
            false,
        );
    }
}
