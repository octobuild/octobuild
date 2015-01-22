pub trait Compiler {
	// Static method signature; `Self` refers to the implementor type
	fn new() -> Self;

	// Filter preprocessed file for precompiled header support.
	fn filter_preprocessed(&self, input: &[u8], marker: &Option<String>, keep_headers: bool) -> Result<Vec<u8>, String>;
}