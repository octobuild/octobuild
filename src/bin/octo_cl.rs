extern crate octobuild;

use octobuild::simple::simple_compile;
use octobuild::vs::compiler::VsCompiler;
use std::process;

fn main() {
    process::exit(simple_compile("cl.exe", |_| VsCompiler::default()))
}
