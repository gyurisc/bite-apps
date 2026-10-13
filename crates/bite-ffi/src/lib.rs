//! C interface to [`bite_engine`] for callers outside Rust, such as the
//! SwiftUI app. The matching C declarations are in `include/bite_ffi.h`; keep
//! the two in step by hand.
//!
//! The boundary is one call: [`bite_engine_dispatch`] hands a command ID and
//! named parameters to [`Engine::dispatch`] and reports the outcome as a
//! [`BiteReply`]. Strings cross as UTF-8 bytes with an explicit length, so
//! document text may contain NUL bytes. Bad input produces a failed reply.

use bite_engine::{Document, Engine};
use std::{ptr, slice, str};

/// An engine instance, opaque to C. Use it from one thread at a time.
pub struct BiteEngine(Engine);

/// Borrowed UTF-8 bytes, not NUL-terminated. `ptr` may be null when `len` is 0.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BiteStr {
    pub ptr: *const u8,
    pub len: usize,
}

impl BiteStr {
    /// Returns `None` for a null pointer with a non-zero length and for bytes
    /// that are not UTF-8.
    ///
    /// # Safety
    ///
    /// `ptr` must be null or point to `len` readable bytes that stay valid
    /// for `'a`.
    unsafe fn as_str<'a>(self) -> Option<&'a str> {
        if self.len == 0 {
            return Some("");
        }
        if self.ptr.is_null() {
            return None;
        }
        str::from_utf8(unsafe { slice::from_raw_parts(self.ptr, self.len) }).ok()
    }
}

/// One named command parameter.
#[repr(C)]
pub struct BiteParam {
    pub name: BiteStr,
    pub value: BiteStr,
}

/// The outcome of one command. Release it with [`bite_reply_free`].
#[repr(C)]
pub struct BiteReply {
    pub ok: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    /// The affected document's ID when `ok`, otherwise 0.
    pub document: u64,
    /// UTF-8: the document text when `ok`, otherwise an error message.
    pub text: *mut u8,
    pub text_len: usize,
}

impl BiteReply {
    fn new(ok: bool, document: u64, can_undo: bool, can_redo: bool, text: &str) -> Self {
        let bytes: Box<[u8]> = text.as_bytes().into();
        let text_len = bytes.len();
        let text = Box::into_raw(bytes).cast::<u8>();
        Self {
            ok,
            can_undo,
            can_redo,
            document,
            text,
            text_len,
        }
    }

    fn document(document: &Document) -> Self {
        Self::new(
            true,
            document.id().into(),
            document.can_undo(),
            document.can_redo(),
            document.text(),
        )
    }

    fn error(message: &str) -> Self {
        Self::new(false, 0, false, false, message)
    }
}

/// Creates an engine. Release it with [`bite_engine_free`].
#[unsafe(no_mangle)]
pub extern "C" fn bite_engine_new() -> *mut BiteEngine {
    Box::into_raw(Box::new(BiteEngine(Engine::new())))
}

/// Destroys an engine. A null `engine` is ignored.
///
/// # Safety
///
/// `engine` must be null or a pointer from [`bite_engine_new`] that has not
/// been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bite_engine_free(engine: *mut BiteEngine) {
    if !engine.is_null() {
        drop(unsafe { Box::from_raw(engine) });
    }
}

/// Runs the command `command` with `param_count` named parameters and
/// reports its outcome.
///
/// # Safety
///
/// - `engine` must be null or a live pointer from [`bite_engine_new`] that no
///   other thread is using.
/// - `command` and each parameter name and value must be null or point to
///   readable bytes of the stated length.
/// - `params` must point to `param_count` parameters; it may be null when
///   `param_count` is 0.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bite_engine_dispatch(
    engine: *mut BiteEngine,
    command: BiteStr,
    params: *const BiteParam,
    param_count: usize,
) -> BiteReply {
    let Some(engine) = (unsafe { engine.as_mut() }) else {
        return BiteReply::error("engine pointer is null");
    };
    let Some(command) = (unsafe { command.as_str() }) else {
        return BiteReply::error("command ID is not a valid UTF-8 string");
    };
    let params = if param_count == 0 {
        &[]
    } else if params.is_null() {
        return BiteReply::error("parameter list is null");
    } else {
        unsafe { slice::from_raw_parts(params, param_count) }
    };

    let mut pairs = Vec::with_capacity(params.len());
    for param in params {
        let name = unsafe { param.name.as_str() };
        let value = unsafe { param.value.as_str() };
        let (Some(name), Some(value)) = (name, value) else {
            return BiteReply::error("a parameter is not a valid UTF-8 string");
        };
        pairs.push((name, value));
    }

    match engine.0.dispatch(command, &pairs) {
        Ok(document) => BiteReply::document(document),
        Err(error) => BiteReply::error(&error.to_string()),
    }
}

/// Releases the text a reply owns.
///
/// # Safety
///
/// `reply` must come from [`bite_engine_dispatch`] and must not have been
/// freed already.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bite_reply_free(reply: BiteReply) {
    if !reply.text.is_null() {
        let text = ptr::slice_from_raw_parts_mut(reply.text, reply.text_len);
        drop(unsafe { Box::from_raw(text) });
    }
}
