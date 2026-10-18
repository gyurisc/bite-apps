//! C interface to [`bite_engine`] for callers outside Rust, such as the
//! SwiftUI app. The matching C declarations are in `include/bite_ffi.h`; keep
//! the two in step by hand.
//!
//! The boundary is one call: [`bite_engine_dispatch`] hands a command ID and
//! named parameters to [`Engine::dispatch`] and reports the outcome as a
//! [`BiteReply`]. Strings cross as UTF-8 bytes with an explicit length, so
//! document text may contain NUL bytes. Bad input produces a failed reply.
//! Bytes that are not UTF-8 are checked with [`text_from_utf8`], so a host
//! can pass a file's bytes to `document.load` unchanged and learn the offset
//! of the first invalid byte, as
//! `docs/architecture/0006-documents-in-the-app.md` records.

use bite_engine::{Engine, Outcome, text_from_utf8};
use std::ops::Range;
use std::{ptr, slice};

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
    /// Fails for a null pointer with a non-zero length, and for bytes that
    /// are not UTF-8, with the offset of the first invalid byte.
    ///
    /// # Safety
    ///
    /// `ptr` must be null or point to `len` readable bytes that stay valid
    /// for `'a`.
    unsafe fn as_str<'a>(self) -> Result<&'a str, String> {
        if self.len == 0 {
            return Ok("");
        }
        if self.ptr.is_null() {
            return Err("null pointer with a non-zero length".to_owned());
        }
        text_from_utf8(unsafe { slice::from_raw_parts(self.ptr, self.len) })
            .map_err(|error| error.to_string())
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
    /// Whether the command changed the text. False for errors and for
    /// `document.close`.
    pub changed: bool,
    /// Whether the document has changes since it was last marked as saved
    /// with `document.mark_saved`, or since it was created or loaded. False
    /// for errors and for `document.close`.
    pub modified: bool,
    /// The affected document's ID when `ok`, otherwise 0.
    pub document: u64,
    /// UTF-8: the document text when `ok`, otherwise an error message. Empty
    /// after `document.close`, which also reports no undo or redo.
    pub text: *mut u8,
    pub text_len: usize,
    /// When `changed`, the byte range of `text` that differs from the text
    /// before the command; an empty range marks where text was removed.
    /// Otherwise both 0.
    pub changed_start: usize,
    pub changed_end: usize,
}

// The layout `include/bite_ffi.h` declares. Update both together.
const _: () = {
    use std::mem::{offset_of, size_of};
    assert!(size_of::<BiteReply>() == 48);
    assert!(offset_of!(BiteReply, ok) == 0);
    assert!(offset_of!(BiteReply, can_undo) == 1);
    assert!(offset_of!(BiteReply, can_redo) == 2);
    assert!(offset_of!(BiteReply, changed) == 3);
    assert!(offset_of!(BiteReply, modified) == 4);
    assert!(offset_of!(BiteReply, document) == 8);
    assert!(offset_of!(BiteReply, text) == 16);
    assert!(offset_of!(BiteReply, text_len) == 24);
    assert!(offset_of!(BiteReply, changed_start) == 32);
    assert!(offset_of!(BiteReply, changed_end) == 40);
};

impl BiteReply {
    fn new(
        ok: bool,
        document: u64,
        can_undo: bool,
        can_redo: bool,
        changed: Option<Range<usize>>,
        modified: bool,
        text: &str,
    ) -> Self {
        let bytes: Box<[u8]> = text.as_bytes().into();
        let text_len = bytes.len();
        let text = Box::into_raw(bytes).cast::<u8>();
        let (changed_start, changed_end) = changed
            .as_ref()
            .map_or((0, 0), |range| (range.start, range.end));
        Self {
            ok,
            can_undo,
            can_redo,
            changed: changed.is_some(),
            modified,
            document,
            text,
            text_len,
            changed_start,
            changed_end,
        }
    }

    /// A closed document replies with its ID, empty text, no undo or redo,
    /// no change, and no unsaved changes, as ADRs 0004, 0005, and 0006
    /// record.
    fn outcome(outcome: Outcome) -> Self {
        match outcome {
            Outcome::Document { document, changed } => Self::new(
                true,
                document.id().into(),
                document.can_undo(),
                document.can_redo(),
                changed,
                document.is_modified(),
                document.text(),
            ),
            Outcome::Closed(id) => Self::new(true, id.into(), false, false, None, false, ""),
        }
    }

    fn error(message: &str) -> Self {
        Self::new(false, 0, false, false, None, false, message)
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
    let command = match unsafe { command.as_str() } {
        Ok(command) => command,
        Err(error) => return BiteReply::error(&format!("command ID: {error}")),
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
        let name = match unsafe { param.name.as_str() } {
            Ok(name) => name,
            Err(error) => return BiteReply::error(&format!("a parameter name: {error}")),
        };
        let value = match unsafe { param.value.as_str() } {
            Ok(value) => value,
            Err(error) => return BiteReply::error(&format!("parameter {name:?}: {error}")),
        };
        pairs.push((name, value));
    }

    match engine.0.dispatch(command, &pairs) {
        Ok(outcome) => BiteReply::outcome(outcome),
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
