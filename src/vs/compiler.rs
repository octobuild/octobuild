pub use ::compiler::Compiler;
use super::postprocess;

#[derive(Copy)]
pub struct VsCompiler;

impl Compiler for VsCompiler {
	fn new() -> Self {
		VsCompiler
	}

	fn filter_preprocessed(&self, input: &[u8], marker: &Option<String>, keep_headers: bool) -> Result<Vec<u8>, String> {
		postprocess::filter_preprocessed(input, marker, keep_headers)
	}
}
