use octobuild::simple::simple_compile;
use octobuild::vs::compiler::VsCompiler;
use std::process;
use std::sync::Arc;
use tempdir::TempDir;

fn main() {
    process::exit(simple_compile("cl.exe", |_| {
        Ok(VsCompiler::new(&Arc::new(TempDir::new("octobuild")?)))
    }))
}
