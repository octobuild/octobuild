use std::io::IoError;

// Scope of command line argument.
#[derive(Copy)]
#[derive(Show)]
#[derive(PartialEq)]
pub enum Scope {
	// Preprocessing argument
	Preprocessor,
	// Compiler argument
	Compiler,
	// Preprocessor & compiler argument
	Shared,
	// Unknown argument - local build only
	Ignore,
}

#[derive(Copy)]
#[derive(Show)]
#[derive(PartialEq)]
pub enum InputKind {
	Source,
	Marker,
	Precompiled,
}

#[derive(Copy)]
#[derive(Show)]
#[derive(PartialEq)]
pub enum OutputKind {
	Object,
	Marker,
}

#[derive(Show)]
#[derive(PartialEq)]
pub enum Arg {
	Flag{scope:Scope, flag: String},
	Param{scope:Scope, flag: String, value: String},
	Input{kind:InputKind, flag: String, file: String},
	Output{kind:OutputKind, flag: String, file: String}
}

#[derive(Show)]
pub struct CompilationTask {
	// Parsed arguments.
	pub args: Vec<Arg>,
	// Source language.
	pub language: String,
	// Input source file name.
	pub input_source: Path,
	// Input precompiled header file name.
	pub input_precompiled: Option<Path>,
	// Output object file name.
	pub output_object: Path,
	// Output precompiled header file name.
	pub output_precompiled: Option<Path>,
	// Marker for precompiled header.
	pub marker_precompiled: Option<String>,
}

pub struct PreprocessResult {
	// Hash
	pub hash: String,
	// Preprocessed file
	pub content: Vec<u8>,
}

pub trait Compiler {
	// Preprocessing source file.
	fn preprocess(&self, task: &CompilationTask) -> Result<PreprocessResult, IoError>;
}