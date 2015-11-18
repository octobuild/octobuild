extern crate octobuild;
extern crate tempdir;

use octobuild::clang::compiler::ClangCompiler;
use octobuild::compiler::*;
use octobuild::cache::Cache;

use tempdir::TempDir;

use std::collections::HashMap;
use std::env;
use std::io;
use std::io::{Error, Write};
use std::iter::FromIterator;
use std::path::Path;
use std::sync::Arc;
use std::process;

fn main() {
	process::exit(match compile() {
		Ok(output) => {
			match output.status {
				Some(r) => r,
				None => 501
			}
		}
		Err(e) => {
			println!("FATAL ERROR: {:?}", e);
			500
		}
	})
}

fn compile() -> Result<OutputInfo, Error> {
	let temp_dir = try! (TempDir::new("octobuild"));
	let compiler = ClangCompiler::new(&Cache::new(), temp_dir.path());
	let args = Vec::from_iter(env::args());
	let output = try! (compiler.compile(CommandInfo {
		program: Path::new("cl.exe").to_path_buf(),
		current_dir: None,
		env: Arc::new(HashMap::new()),
	}, &args[1..]));

	try !(io::stdout().write_all(&output.stdout));
	try !(io::stderr().write_all(&output.stderr));
	Ok(output)
}
