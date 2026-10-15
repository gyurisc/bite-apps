//! Saving through a temporary file and a rename, so the file at the path
//! holds either its old contents or the new ones, never a partial write.

use std::ffi::OsString;
use std::fs::{self, File, OpenOptions, Permissions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process;

/// Replaces the contents of the file at `path` with `bytes`, creating the
/// file if it does not exist. If this fails, the file is unchanged.
///
/// A symbolic link is followed and kept. An existing file keeps its
/// permissions and is only replaced if it could be written to directly.
pub fn save(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let target = match fs::canonicalize(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => path.to_path_buf(),
        result => result?,
    };
    let permissions = match fs::metadata(&target) {
        Ok(metadata) => {
            OpenOptions::new().write(true).open(&target)?;
            Some(metadata.permissions())
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };

    let (temp, file) = create_temp(&target)?;
    let saved = fill(file, bytes, permissions).and_then(|()| fs::rename(&temp, &target));
    if saved.is_err() {
        let _ = fs::remove_file(&temp);
    }
    saved
}

/// Creates a new, empty file beside `target`.
fn create_temp(target: &Path) -> io::Result<(PathBuf, File)> {
    let Some(name) = target.file_name() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the path does not name a file",
        ));
    };
    for attempt in 0..100 {
        let mut temp_name = OsString::from(".");
        temp_name.push(name);
        temp_name.push(format!(".bite-{}-{attempt}.tmp", process::id()));
        let temp = target.with_file_name(temp_name);
        match OpenOptions::new().write(true).create_new(true).open(&temp) {
            Ok(file) => return Ok((temp, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "no free temporary file name",
    ))
}

fn fill(mut file: File, bytes: &[u8], permissions: Option<Permissions>) -> io::Result<()> {
    file.write_all(bytes)?;
    if let Some(permissions) = permissions {
        file.set_permissions(permissions)?;
    }
    file.sync_all()
}
