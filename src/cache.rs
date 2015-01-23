extern crate "sha1-hasher" as sha1;

use std::os;
use std::io::fs;
use std::io::{File, IoError, IoErrorKind, Reader, Writer, USER_RWX};
use std::io::process::{ProcessOutput, ProcessExit};

const HEADER: &'static [u8] = b"OBCF\x00\x01";
const FOOTER: &'static [u8] = b"END\x00";

pub struct Cache {
	cache_dir: Path
}

impl Cache {
	pub fn new() -> Self {
		let cache_dir = os::homedir().unwrap().join_many(&[".octobuild", "cache"]);
		Cache {
			cache_dir: cache_dir
		}
	}

	pub fn run_cached<F: Fn()->Result<ProcessOutput, IoError>>(&self, params: &str, inputs: &Vec<Path>, outputs: &Vec<Path>, worker: F) -> Result<ProcessOutput, IoError> {
		let hash = try! (generate_hash(params, inputs));
		let path = self.cache_dir.join(hash.slice(0, 2)).join(hash.slice(2, 4)).join(hash.slice_from(4));
		println!("Cache file: {:?}", path);
		// Try to read data from cache.
		match read_cache(&path, outputs) {
			Ok(output) => {return Ok(output)}
			Err(_) => {}
		}
		// Run task and save result to cache.
		let output = try !(worker());
		try !(write_cache(&path, outputs, &output));
		Ok(output)
	}
}

// @todo: Need more safe data writing (size before data).
fn generate_hash(params: &str, inputs: &Vec<Path>) -> Result<String, IoError> {
	use std::hash::Writer;

	let mut hash = sha1::Sha1::new();
	// str
	hash.write(params.as_bytes());
	hash.write(&[0]);
	// inputs
	for input in inputs.iter() {
		let content = try! (File::open(input).read_to_end());
		hash.write(content.as_slice());
		hash.write(&[0]);
	}
	Ok(hash.hexdigest())
}

fn write_cache(path: &Path, paths: &Vec<Path>, output: &ProcessOutput) -> Result<(), IoError> {
	if !output.status.success() {
		return Ok(());
	}
	try! (fs::mkdir_recursive(&path.dir_path(), USER_RWX));
	let mut stream = try! (File::create(path));
	try! (stream.write(HEADER));
	try! (stream.write_le_uint(paths.len()));
	for path in paths.iter() {
		let content = try! (File::open(path).read_to_end());
		try! (write_blob(&mut stream, content.as_slice()));	
	}
	try! (write_output(&mut stream, output));
	try! (stream.write(FOOTER));
	try! (stream.flush());
	Ok(())
}

fn read_cache(path: &Path, paths: &Vec<Path>) -> Result<ProcessOutput, IoError> {
	let mut stream = try! (File::open(path));
	if try! (stream.read_exact(HEADER.len())) != HEADER {
		return Err(IoError {
			kind: IoErrorKind::InvalidInput,
			desc: "Invalid cache file header",
			detail: Some(path.display().to_string())
		})
	}
	if try! (stream.read_le_uint()) != paths.len() {
		return Err(IoError {
			kind: IoErrorKind::InvalidInput,
			desc: "Unexpected count of packed cached files",
			detail: Some(path.display().to_string())
		})
	} 
	for path in paths.iter() {
		let content = try! (read_blob(&mut stream));
		try! (File::create(path).write(content.as_slice()));		
	}
	let output = try! (read_output(&mut stream));
	if try! (stream.read_exact(FOOTER.len())) != FOOTER {
		return Err(IoError {
			kind: IoErrorKind::InvalidInput,
			desc: "Invalid cache file footer",
			detail: Some(path.display().to_string())
		})
	}
	Ok(output)
}

fn write_blob(stream: &mut Writer, blob: &[u8]) -> Result<(), IoError> {
	try! (stream.write_le_uint(blob.len()));
	try! (stream.write(blob));
	Ok(())
}

fn read_blob(stream: &mut Reader) -> Result<Vec<u8>, IoError> {
	let size = try! (stream.read_le_uint());
	stream.read_exact(size)
}

fn write_output(stream: &mut Writer, output: &ProcessOutput) -> Result<(), IoError> {
	try! (write_blob(stream, output.output.as_slice()));
	try! (write_blob(stream, output.error.as_slice()));
	Ok(())
}

fn read_output(stream: &mut Reader) -> Result<ProcessOutput, IoError> {
	let output = try! (read_blob(stream));
	let error  = try! (read_blob(stream));
	Ok(ProcessOutput {
		status: ProcessExit::ExitStatus(0),
		output: output,
		error: error,
	})
}
