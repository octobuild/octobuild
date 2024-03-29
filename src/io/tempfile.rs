use std::fs;
use std::io::Error;
use std::path::{Path, PathBuf};

use uuid::Uuid;

pub struct TempFile {
    path: Option<PathBuf>,
    disarmed: bool,
}

impl TempFile {
    /// Create random file name in specified directory.
    #[must_use]
    pub fn new_in(path: &Path, suffix: &str) -> Self {
        let random_name = Uuid::new_v4().to_string() + suffix;
        TempFile::wrap(&path.join(random_name))
    }

    /// Wrap path to a temporary file. The file will be automatically
    /// deleted once the returned wrapper is destroyed.
    ///
    /// If no directory can be created, `Err` is returned.
    #[must_use]
    pub fn wrap(path: &Path) -> TempFile {
        TempFile {
            path: Some(path.to_path_buf()),
            disarmed: false,
        }
    }

    /// Access the wrapped `std::path::Path` to the temporary file.
    #[must_use]
    pub fn path(&self) -> &Path {
        self.path.as_ref().unwrap()
    }

    fn cleanup_file(&mut self) -> Result<(), Error> {
        assert!(!self.disarmed);
        self.disarmed = true;
        match self.path {
            Some(ref p) => fs::remove_file(p),
            None => Ok(()),
        }
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        if !self.disarmed {
            drop(self.cleanup_file());
        }
    }
}
