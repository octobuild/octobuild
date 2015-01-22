extern crate uuid;

use std::io::fs;
use std::io::IoResult;

pub struct TempFile {
	path: Option<Path>,
	disarmed: bool
}

impl TempFile {
	/// Create random file name in specified directory.
	pub fn new_in(path: &Path, suffix: &str) -> TempFile {
		TempFile::wrap(path.join(Path::new(uuid::Uuid::new_v4().to_string() + suffix)))
	}

	/// Wrap path to a temporary file. The file will be automatically
	/// deleted once the returned wrapper is destroyed.
	///
	/// If no directory can be created, `Err` is returned.
	pub fn wrap(path: Path) -> TempFile {
		TempFile{
			path: Some(path),
			disarmed: false
		}
	}

	/// Access the wrapped `std::path::Path` to the temporary file.
	pub fn path<'a>(&'a self) -> &'a Path {
		self.path.as_ref().unwrap()
	}

	fn cleanup_file(&mut self) -> IoResult<()> {
		assert!(!self.disarmed);
		self.disarmed = true;
		match self.path {
			Some(ref p) => fs::unlink(p),
	    None => Ok(())
		}
	}
}

impl Drop for TempFile {
	fn drop(&mut self) {
		if !self.disarmed {
			let _ = self.cleanup_file();
		}
	}
}