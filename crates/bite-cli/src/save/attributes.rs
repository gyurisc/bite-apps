//! What a save carries over to the new file besides its text, mode, and
//! group, using macOS system functions.

use std::ffi::{CString, c_char, c_int, c_uint, c_void};
use std::fs::File;
use std::io;
use std::os::fd::AsRawFd;
use std::ptr;

// From <sys/errno.h> and <xattr_flags.h>.
const ERANGE: i32 = 34;
const ENOTSUP: i32 = 45;
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

/// Removes every access control list entry from `file`, including entries it
/// inherited from its folder when it was created.
pub fn clear_acl(file: &File) -> io::Result<()> {
    // SAFETY: `acl_init` has no preconditions.
    let empty = unsafe { acl_init(0) };
    if empty.is_null() {
        return Err(context("access control list", io::Error::last_os_error()));
    }
    match set_acl(file, empty) {
        Err(error) if !unsupported(&error) => Err(context("access control list", error)),
        _ => Ok(()),
    }
}

/// Gives `to` exactly the access control list of `from`.
pub fn copy_acl(from: &File, to: &File) -> io::Result<()> {
    // SAFETY: the descriptor is open for the length of the call.
    let acl = unsafe { acl_get_fd(from.as_raw_fd()) };
    if acl.is_null() {
        let error = io::Error::last_os_error();
        // `from` has no list, or its volume does not support them.
        if error.kind() == io::ErrorKind::NotFound || unsupported(&error) {
            return clear_acl(to);
        }
        return Err(context("access control list", error));
    }
    set_acl(to, acl).map_err(|error| context("access control list", error))
}

/// Sets the list of `file` to `acl` and frees `acl`.
fn set_acl(file: &File, acl: *mut c_void) -> io::Result<()> {
    // SAFETY: the descriptor is open, and `acl` came from `acl_init` or
    // `acl_get_fd` and is freed only here, after its last use.
    let result = unsafe { acl_set_fd(file.as_raw_fd(), acl) };
    let error = io::Error::last_os_error();
    unsafe { acl_free(acl) };
    if result == 0 { Ok(()) } else { Err(error) }
}

/// Copies each extended attribute of `from` that macOS keeps when it saves a
/// document, such as Finder tags, to `to`. Attributes that describe the old
/// contents, such as the text encoding TextEdit records, are left behind.
pub fn copy_extended_attributes(from: &File, to: &File) -> io::Result<()> {
    let names = sized(|buffer, size| {
        // SAFETY: `buffer` is null with `size` 0, or holds `size` bytes.
        unsafe { flistxattr(from.as_raw_fd(), buffer.cast(), size, 0) }
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
        let value = sized(|buffer, size| {
            // SAFETY: as above; `name` outlives the call.
            unsafe { fgetxattr(from.as_raw_fd(), name.as_ptr(), buffer.cast(), size, 0, 0) }
        })
        .map_err(|error| context("extended attributes", error))?;
        // SAFETY: `value` holds `value.len()` bytes for the length of the call.
        let result = unsafe {
            fsetxattr(
                to.as_raw_fd(),
                name.as_ptr(),
                value.as_ptr().cast(),
                value.len(),
                0,
                0,
            )
        };
        if result != 0 {
            return Err(context("extended attributes", io::Error::last_os_error()));
        }
    }
    Ok(())
}

/// Calls `read` the way the extended-attribute functions expect: first with
/// no buffer to learn the size, then with a buffer of that size, and again if
/// the data grew in between.
fn sized(mut read: impl FnMut(*mut u8, usize) -> isize) -> io::Result<Vec<u8>> {
    for _ in 0..10 {
        let Ok(size) = usize::try_from(read(ptr::null_mut(), 0)) else {
            return Err(io::Error::last_os_error());
        };
        let mut buffer = vec![0; size];
        match usize::try_from(read(buffer.as_mut_ptr(), size)) {
            Ok(read) => {
                buffer.truncate(read);
                return Ok(buffer);
            }
            Err(_) => {
                let error = io::Error::last_os_error();
                if error.raw_os_error() != Some(ERANGE) {
                    return Err(error);
                }
            }
        }
    }
    Err(io::Error::other("the data kept changing size"))
}

fn unsupported(error: &io::Error) -> bool {
    matches!(error.raw_os_error(), Some(ENOTSUP | EOPNOTSUPP))
}

fn context(what: &str, error: io::Error) -> io::Error {
    io::Error::new(error.kind(), format!("cannot keep its {what}: {error}"))
}
