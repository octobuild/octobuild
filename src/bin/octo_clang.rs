use octobuild::clang::compiler::ClangCompiler;
use octobuild::simple::simple_compile;
use std::process;

fn main() {
    env_logger::init();
    
    process::exit(simple_compile("clang", |_| Ok(ClangCompiler::default())))
}
