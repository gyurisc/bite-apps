//! Saving through a temporary file and a rename, so the file at the path
//! holds either its old contents or the new ones, never a partial write.

use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt, fchown};
use std::path::{Path, PathBuf};
use std::process;

/// Replaces the contents of the file at `path` with `bytes`, creating the
/// file if it does not exist. If this fails, the file is unchanged.
///
/// A symbolic link is followed and kept; a link to a missing file is refused.
/// An existing file is replaced only if it could be written to directly, and
/// not if it has other hard links, because replacing it would separate them.
///
/// The replacement keeps the file's mode and group. If the group cannot be
/// kept, the new group gets no more access than others have. On macOS
/// it also keeps the access control list, exactly, and the extended
/// attributes macOS keeps when it saves a document, such as Finder tags, but
/// not the quarantine flag of a downloaded file. It does not keep the owner,
/// the creation date, or file flags such as hidden.
pub fn save(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let Pending {
        target,
        original,
        temp,
        file,
    } = begin(path)?;
    let saved = fill(&file, bytes, original.as_ref()).and_then(|()| fs::rename(&temp, &target));
    if saved.is_err() {
        // The original's access control list may forbid deleting the copy.
        let _ = attributes::clear_acl(&file);
        let _ = fs::remove_file(&temp);
    }
    saved
}

/// A save that has checked its target and created the empty temporary file
/// that will replace it.
struct Pending {
    target: PathBuf,
    /// The file being replaced, if it exists, open for reading and writing.
    original: Option<File>,
    temp: PathBuf,
    file: File,
}

fn begin(path: &Path) -> io::Result<Pending> {
    let target = resolve(path)?;
    let original = match OpenOptions::new().read(true).write(true).open(&target) {
        Ok(file) => Some(file),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    if let Some(original) = &original {
        let links = original.metadata()?.nlink();
        if links > 1 {
            return Err(io::Error::other(format!(
                "it has {links} hard links, and replacing it would separate them"
            )));
        }
    }

    // Until it holds the original's permissions, the new text stays private:
    // the file is created readable only by its owner, without the access
    // control list entries it would inherit from the folder.
    let (temp, file) = create_temp(&target, original.is_some())?;
    if original.is_some()
        && let Err(error) = attributes::clear_acl(&file)
    {
        let _ = fs::remove_file(&temp);
        return Err(error);
    }
    Ok(Pending {
        target,
        original,
        temp,
        file,
    })
}

/// The file that saving to `path` replaces: the file a symbolic link points
/// to, or `path` itself.
fn resolve(path: &Path) -> io::Result<PathBuf> {
    match fs::canonicalize(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let is_link = fs::symlink_metadata(path).is_ok_and(|meta| meta.is_symlink());
            if is_link {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "it is a symbolic link to a missing file",
                ));
            }
            Ok(path.to_path_buf())
        }
        result => result,
    }
}

/// Creates a new, empty file beside `target`, readable only by its owner if
/// `private`.
fn create_temp(target: &Path, private: bool) -> io::Result<(PathBuf, File)> {
    let Some(name) = target.file_name() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the path does not name a file",
        ));
    };
    let mode = if private { 0o600 } else { 0o666 };
    for attempt in 0..100 {
        let mut temp_name = OsString::from(".");
        temp_name.push(name);
        temp_name.push(format!(".bite-{}-{attempt}.tmp", process::id()));
        let temp = target.with_file_name(temp_name);
        let created = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(mode)
            .open(&temp);
        match created {
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

/// Writes `bytes` to the temporary file and, when it replaces `original`,
/// gives it what the original has besides its text.
fn fill(mut file: &File, bytes: &[u8], original: Option<&File>) -> io::Result<()> {
    file.write_all(bytes)?;
    if let Some(original) = original {
        keep_metadata(original, file)?;
    }
    file.sync_all()
}

fn keep_metadata(original: &File, file: &File) -> io::Result<()> {
    // Extended attributes come first: the original's access control list may
    // stop even its owner from setting them.
    attributes::copy_extended_attributes(original, file)?;
    let metadata = original.metadata()?;
    let mut mode = metadata.mode() & 0o7777;
    if file.metadata()?.gid() != metadata.gid()
        && let Err(error) = fchown(file, None, Some(metadata.gid()))
    {
        if error.kind() != io::ErrorKind::PermissionDenied {
            let message = format!("cannot keep its group: {error}");
            return Err(io::Error::new(error.kind(), message));
        }
        // The user is not in the original's group, so the file keeps the
        // folder's group instead. Members of a file's group get only the group
        // bits, so they get what others have and nothing more.
        let others = mode & 0o007;
        mode = (mode & !0o070) | ((mode >> 3) & others) << 3;
    }
    // The access control list, with any deny entries, is in place before the
    // mode opens the file up. The mode comes after the group change, which
    // can clear set-ID bits.
    attributes::copy_acl(original, file)?;
    file.set_permissions(fs::Permissions::from_mode(mode))
}

#[cfg(target_os = "macos")]
mod attributes;

/// Other systems keep only the mode and group.
#[cfg(not(target_os = "macos"))]
mod attributes {
    use std::fs::File;
    use std::io;

    pub fn clear_acl(_file: &File) -> io::Result<()> {
        Ok(())
    }

    pub fn copy_acl(_from: &File, _to: &File) -> io::Result<()> {
        Ok(())
    }

    pub fn copy_extended_attributes(_from: &File, _to: &File) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn the_new_text_for_an_existing_file_goes_into_a_private_file() {
        let dir = std::env::temp_dir().join(format!("bite-cli-save-{}", process::id()));
        fs::create_dir_all(&dir).unwrap();
        let probe = dir.join("probe.txt");
        fs::write(&probe, "").unwrap();
        if fs::metadata(&probe).unwrap().permissions().mode() & 0o777 == 0o600 {
            fs::remove_dir_all(&dir).unwrap();
            eprintln!("skipped: the umask already makes every new file private");
            return;
        }
        let path = dir.join("notes.txt");
        fs::write(&path, "old").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();

        let pending = begin(&path).unwrap();
        let mode = pending.file.metadata().unwrap().permissions().mode() & 0o777;
        fs::remove_dir_all(&dir).unwrap();

        assert_eq!(mode, 0o600, "{}", pending.temp.display());
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn the_new_text_for_an_existing_file_gets_no_access_from_the_folder() {
        let dir = std::env::temp_dir().join(format!("bite-cli-save-acl-{}", process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("notes.txt");
        fs::write(&path, "old").unwrap();
        let inherit = std::process::Command::new("chmod")
            .args(["+a", "user:daemon allow read,file_inherit"])
            .arg(&dir)
            .status()
            .unwrap();
        assert!(inherit.success());

        let pending = begin(&path).unwrap();
        let listing = std::process::Command::new("ls")
            .arg("-le")
            .arg(&pending.temp)
            .output()
            .unwrap();
        fs::remove_dir_all(&dir).unwrap();

        let listing = String::from_utf8(listing.stdout).unwrap();
        assert!(!listing.contains("daemon"), "{listing}");
    }
}
