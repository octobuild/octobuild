#![allow(unstable)]
extern crate octobuild;
extern crate log;

use octobuild::vs::compiler::VsCompiler;
use octobuild::compiler::Compiler;

use std::os;
use std::io::TempDir;

fn main() {
	let temp_dir = match TempDir::new("octobuild") {
		Ok(result) => result,
		Err(e) => {panic!(e);}
	};
	let compiler = VsCompiler::new(temp_dir.path());

	let result = compiler.create_task(&os::args()[1..]);
	println!("Parsed task: {:?}", result);
	match result {
			Ok(task) => {
				match compiler.preprocess(&task) {
					Ok(result) => {
						compiler.compile(&task, result);
					}
					Err(e) => {
							panic!(e);
					}
					}
		}
			_ => {}
		}

	/*match Command::new("cl.exe")
	.args(os::args()[1..].as_slice())
	.output(){
			Ok(output) => {
			println!("stdout: {}", String::from_utf8_lossy(output.output.as_slice()));
			println!("stderr: {}", String::from_utf8_lossy(output.error.as_slice()));
		}
			Err(e) => {
			panic!("{}", e);
		}
		}*/
}
