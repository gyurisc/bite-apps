//! What a save carries over to the new file besides its text, mode, and
//! group, using macOS system functions.

use std::ffi::{CStr, CString, c_char, c_int, c_uint, c_void};
use std::fs::File;
use std::io;
use std::os::fd::AsRawFd;
use std::ptr;

// From <sys/errno.h> and <xattr_flags.h>.
const EPERM: i32 = 1;
const ERANGE: i32 = 34;
const ENOTSUP: i32 = 45;
const ENOATTR: i32 = 93;
const EOPNOTSUPP: i32 = 102;
const XATTR_OPERATION_INTENT_SAVE: c_uint = 2;

// From <sys/xattr.h>, <xattr_flags.h>, and <sys/acl.h>; `acl_t` is a pointer.
unsafe extern "C" {
    fn flistxattr(fd: c_int, names: *mut c_char, size: usize, options: c_int) -> isize;
    fn fgetxattr(
        fd: c_int,
        name: *const c_char,
        value: *mut c_void,
        size: usize,
        position: u32,
        options: c_int,
    ) -> isize;
    fn fsetxattr(
        fd: c_int,
        name: *const c_char,
        value: *const c_void,
        size: usize,
        position: u32,
        options: c_int,
    ) -> c_int;
    fn xattr_preserve_for_intent(name: *const c_char, intent: c_uint) -> c_int;
    fn acl_get_fd(fd: c_int) -> *mut c_void;
    fn acl_set_fd(fd: c_int, acl: *mut c_void) -> c_int;
    fn acl_init(count: c_int) -> *mut c_void;
    fn acl_free(object: *mut c_void) -> c_int;
}

/// An access control list owned by this process, freed when dropped.
struct Acl(*mut c_void);

impl Acl {
    fn empty() -> io::Result<Self> {
        // SAFETY: `acl_init` has no preconditions.
        let acl = unsafe { acl_init(0) };
        if acl.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(acl))
    }

    /// The list of `file`, or the reason it has none: `NotFound` when it has
    /// no list, "not supported" when its volume has no lists.
    fn of(file: &File) -> Result<Self, io::Error> {
        // SAFETY: the descriptor is open for the length of the call.
        let acl = unsafe { acl_get_fd(file.as_raw_fd()) };
        if acl.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(acl))
    }

    fn apply(&self, file: &File) -> io::Result<()> {
        // SAFETY: the descriptor is open and the list is live until dropped.
        if unsafe { acl_set_fd(file.as_raw_fd(), self.0) } == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
}

impl Drop for Acl {
    fn drop(&mut self) {
        // SAFETY: the list came from `acl_init` or `acl_get_fd` and is freed
        // only here.
        unsafe { acl_free(self.0) };
    }
}

/// Removes every access control list entry from `file`, including entries it
/// inherited from its folder when it was created.
pub fn clear_acl(file: &File) -> io::Result<()> {
    let empty = Acl::empty().map_err(|error| context("access control list", error))?;
    match empty.apply(file) {
        Err(error) if !unsupported(&error) => Err(context("access control list", error)),
        _ => Ok(()),
    }
}

/// Gives `to` exactly the access control list of `from`.
pub fn copy_acl(from: &File, to: &File) -> io::Result<()> {
    match Acl::of(from) {
        Ok(acl) => acl
            .apply(to)
            .map_err(|error| context("access control list", error)),
        // `from` has no list, or its volume does not support them.
        Err(error) if error.kind() == io::ErrorKind::NotFound || unsupported(&error) => {
            clear_acl(to)
        }
        Err(error) => Err(context("access control list", error)),
    }
}

/// Copies each extended attribute of `from` that macOS keeps when it saves a
/// document, such as Finder tags, to `to`. Attributes that describe the old
/// contents, such as the text encoding TextEdit records, are left behind, and
/// so is the quarantine flag of a downloaded file.
pub fn copy_extended_attributes(from: &File, to: &File) -> io::Result<()> {
    let names = sized(|buffer, size| {
        // SAFETY: `buffer` is null with `size` 0, or holds `size` bytes.
        length(unsafe { flistxattr(from.as_raw_fd(), buffer.cast(), size, 0) })
    })
    .map_err(|error| context("extended attributes", error))?;

    for name in names
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
    {
        let name = CString::new(name).map_err(io::Error::other)?;
        // SAFETY: `name` is a NUL-terminated string.
        let keep = unsafe { xattr_preserve_for_intent(name.as_ptr(), XATTR_OPERATION_INTENT_SAVE) };
        if keep == 0 {
            continue;
        }
        // An attribute removed since it was listed is not copied.
        let Some(value) = read_attribute(from, &name)? else {
            continue;
        };
        write_attribute(to, &name, &value)?;
    }
    Ok(())
}

/// The value of attribute `name`, or `None` if `file` does not have it.
fn read_attribute(file: &File, name: &CStr) -> io::Result<Option<Vec<u8>>> {
    let value = sized(|buffer, size| {
        // SAFETY: `buffer` is null with `size` 0, or holds `size` bytes, and
        // `name` outlives the call.
        length(unsafe { fgetxattr(file.as_raw_fd(), name.as_ptr(), buffer.cast(), size, 0, 0) })
    });
    match value {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.raw_os_error() == Some(ENOATTR) => Ok(None),
        Err(error) => Err(context("extended attributes", error)),
    }
}

/// Sets attribute `name` on `file`. A `com.apple.` attribute that only the
/// system may set, such as `com.apple.rootless`, is skipped; any other
/// refusal is an error.
fn write_attribute(file: &File, name: &CStr, value: &[u8]) -> io::Result<()> {
    // SAFETY: `value` holds `value.len()` bytes and `name` is NUL-terminated,
    // both for the length of the call.
    let result = unsafe {
        fsetxattr(
            file.as_raw_fd(),
            name.as_ptr(),
            value.as_ptr().cast(),
            value.len(),
            0,
            0,
        )
    };
    if result == 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(EPERM) && name.to_bytes().starts_with(b"com.apple.") {
        return Ok(());
    }
    Err(context("extended attributes", error))
}

/// Turns the result of a function that returns a length or -1 into a result.
fn length(result: isize) -> io::Result<usize> {
    usize::try_from(result).map_err(|_| io::Error::last_os_error())
}

/// Calls `read` the way the extended-attribute functions expect: first with
/// no buffer to learn the size, then with a buffer of that size, and again if
/// the data grew in between.
fn sized(mut read: impl FnMut(*mut u8, usize) -> io::Result<usize>) -> io::Result<Vec<u8>> {
    for _ in 0..10 {
        let size = read(ptr::null_mut(), 0)?;
        let mut buffer = vec![0; size];
        match read(buffer.as_mut_ptr(), size) {
            Ok(read) if read <= size => {
                buffer.truncate(read);
                return Ok(buffer);
            }
            // Grown: a full buffer reports `ERANGE`, an empty one the new size.
            Ok(_) => {}
            Err(error) if error.raw_os_error() == Some(ERANGE) => {}
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::other("the data kept changing size"))
}

/// The volume does not support the operation, such as an access control list
/// on exFAT.
fn unsupported(error: &io::Error) -> bool {
    matches!(error.raw_os_error(), Some(ENOTSUP | EOPNOTSUPP))
}

fn context(what: &str, error: io::Error) -> io::Error {
    io::Error::new(error.kind(), format!("cannot keep its {what}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // macOS errno values, written out so a wrong constant above shows up.
    fn os(code: i32) -> io::Error {
        io::Error::from_raw_os_error(code)
    }

    #[test]
    fn only_not_supported_errors_mean_the_volume_lacks_a_feature() {
        assert!(unsupported(&os(45)));
        assert!(unsupported(&os(102)));
        assert!(!unsupported(&os(13)));
        assert!(!unsupported(&os(2)));
    }

    /// A new file for one test, removed with its folder by `done`.
    fn scratch_file(test: &str) -> (std::path::PathBuf, File) {
        let dir = std::env::temp_dir().join(format!("bite-cli-attr-{test}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("file.txt");
        let file = File::create(&path).unwrap();
        (path, file)
    }

    fn done(path: &std::path::Path) {
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    fn name(name: &str) -> CString {
        CString::new(name).unwrap()
    }

    #[test]
    fn an_attribute_that_is_gone_reads_as_none() {
        let (path, file) = scratch_file("gone");

        let value = read_attribute(&file, &name("com.example.never-set"));
        done(&path);

        assert_eq!(value.unwrap(), None);
    }

    #[test]
    fn a_system_only_attribute_is_skipped_but_other_refusals_fail() {
        let (path, file) = scratch_file("refused");

        // Only the system may set this one, so setting it is refused.
        let system = write_attribute(&file, &name("com.apple.rootless"), b"x");
        let system_value = read_attribute(&file, &name("com.apple.rootless"));
        // An immutable file refuses every attribute.
        let locked = std::process::Command::new("chflags")
            .arg("uchg")
            .arg(&path)
            .status()
            .unwrap();
        let other = write_attribute(&file, &name("com.example.note"), b"x");
        std::process::Command::new("chflags")
            .arg("nouchg")
            .arg(&path)
            .status()
            .unwrap();
        done(&path);

        assert!(system.is_ok(), "{system:?}");
        assert_eq!(system_value.unwrap(), None);
        assert!(locked.success());
        assert_eq!(other.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn data_that_grows_between_the_two_calls_is_read_again() {
        let mut calls = 0;
        let data = sized(|buffer, size| {
            calls += 1;
            match calls {
                1 => Ok(3),
                2 => Err(os(34)),
                3 => Ok(5),
                _ => {
                    assert_eq!(size, 5);
                    // SAFETY: the buffer holds `size` bytes.
                    unsafe { ptr::copy_nonoverlapping(b"grown".as_ptr(), buffer, 5) };
                    Ok(5)
                }
            }
        })
        .unwrap();

        assert_eq!(data, b"grown");
    }

    #[test]
    fn data_that_appears_after_a_size_of_zero_is_read_again() {
        let mut calls = 0;
        let data = sized(|buffer, size| {
            calls += 1;
            match calls {
                1 => Ok(0),
                // Given an empty buffer, the kernel reports the full size.
                2 => Ok(4),
                3 => Ok(4),
                _ => {
                    assert_eq!(size, 4);
                    // SAFETY: the buffer holds `size` bytes.
                    unsafe { ptr::copy_nonoverlapping(b"late".as_ptr(), buffer, 4) };
                    Ok(4)
                }
            }
        })
        .unwrap();

        assert_eq!(data, b"late");
    }

    #[test]
    fn other_read_errors_are_not_retried() {
        let mut calls = 0;
        let result = sized(|_, _| {
            calls += 1;
            if calls == 1 { Ok(3) } else { Err(os(13)) }
        });

        assert_eq!(result.unwrap_err().raw_os_error(), Some(13));
        assert_eq!(calls, 2);
    }
}
