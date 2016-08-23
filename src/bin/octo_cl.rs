extern crate octobuild;

use octobuild::vs::compiler::VsCompiler;
use octobuild::simple::simple_compile;
use std::process;

fn main() {
    process::exit(simple_compile("cl.exe", |_, state| VsCompiler::default(state)))
}
