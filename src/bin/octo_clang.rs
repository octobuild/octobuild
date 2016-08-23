extern crate octobuild;

use octobuild::clang::compiler::ClangCompiler;
use octobuild::simple::simple_compile;
use std::process;

fn main() {
    process::exit(simple_compile("clang", |_| Ok(ClangCompiler::new())))
}
