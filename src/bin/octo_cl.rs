#![allow(unstable)]
extern crate octobuild;
extern crate log;

use octobuild::vs::compiler::VsCompiler;
use octobuild::compiler::Compiler;
use octobuild::cache::Cache;

use std::os;
use std::io::{stderr, stdout, TempDir, Command, IoError};
use std::io::process::{ProcessExit, ProcessOutput};

fn main() {
	match compile() {
		Ok(output) => {
			std::os::set_exit_status(match output.status {
				ProcessExit::ExitStatus(r) => r,
				ProcessExit::ExitSignal(r) => r
			});
		}
		Err(e) => {
			println!("FATAL ERROR: {:?}", e);
			std::os::set_exit_status(500);
		}
	}
}

fn compile() -> Result<ProcessOutput, IoError> {
	let temp_dir = try! (TempDir::new("octobuild"));
	let compiler = VsCompiler::new(&Cache::new(), temp_dir.path());
	let output = try! (compiler.compile(&Command::new("cl.exe"), &os::args()[1..]));

	try !(stdout().write(output.output.as_slice()));
	try !(stderr().write(output.error.as_slice()));
	Ok(output)
}
