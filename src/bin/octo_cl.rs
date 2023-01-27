use octobuild::simple::simple_compile;
use octobuild::vs::compiler::VsCompiler;
use std::process;

fn main() -> std::io::Result<()> {
    process::exit(simple_compile("cl.exe", |_| Ok(VsCompiler::new())))
}
