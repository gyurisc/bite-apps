//! Headless, in-memory plain-text document engine for Bite Apps.
//!
//! Every change to a document goes through a command. [`Engine::dispatch`]
//! takes a command ID and named string parameters, which suits untrusted
//! callers such as a CLI; [`Engine::execute`] takes an already typed
//! [`Command`]. Both run the same code, and both return an [`Outcome`],
//! which is the affected [`Document`] or the ID of a closed one, or an
//! [`EngineError`]. No input makes the engine panic.
//!
//! # Commands
//!
//! Command IDs are stable: an ID keeps its meaning once it is listed here.
//!
//! | ID | Parameters | Effect |
//! |---|---|---|
//! | `document.create` | none | Creates an empty document. |
//! | `document.load` | `text` | Creates a document holding `text`, with no undo history. |
//! | `document.replace_text` | `document`, `text` | Replaces the whole text. |
//! | `document.replace_range` | `document`, `start`, `end`, `text` | Replaces the bytes from `start` up to but not including `end`. |
//! | `document.undo` | `document` | Reverts the most recent undo step. |
//! | `document.redo` | `document` | Reapplies the most recently undone step. |
//! | `document.close` | `document` | Removes the document and its history. Later commands naming it fail as for an unknown document, and its ID is not reused. |
//!
//! `document` is the decimal ID of a document created by this engine. Every
//! listed parameter is required, and a command accepts no others.
//!
//! `start` and `end` are UTF-8 byte offsets into the text, written as ASCII
//! decimal digits with no sign. They must satisfy `start <= end <=` the
//! text's length, and each must fall on a boundary between Unicode scalar
//! values. Replacing text with identical text is not a change.
//!
//! # Undo steps
//!
//! `document.replace_text` records one undo step per change. Range edits
//! group as typing does: an insertion joins the current step when it goes
//! where the previous insertion or replacement ended, and a deletion joins
//! it when it ends or starts where the previous deletion started, as
//! Backspace and Forward Delete do. Any other change, and any change after
//! `document.replace_text`, `document.undo`, or `document.redo`, starts a
//! new step. Rejected commands and commands that change nothing do not
//! affect grouping. Grouping depends only on the order of edits, never on
//! timing; `docs/architecture/0004-edit-model.md` records why.
//!
//! # Changed range
//!
//! [`Outcome::changed`] reports the byte range of the new text that differs
//! from the text before the command, or `None` if the text did not change.
//! The bytes both texts share at the start are left out, back to the last
//! whole Unicode scalar value; then the bytes they share at the end of what
//! remains, forward to the next whole one. An empty range marks where text was only removed;
//! a created or loaded document reports its whole text. Hosts use the range
//! to update a view and select what undo or redo restored, as
//! `docs/architecture/0005-composition-and-changed-range.md` records.
//!
//! # Loading and saving
//!
//! The engine never touches files. To open one, a host reads its bytes,
//! turns them into text with [`text_from_utf8`], and dispatches
//! `document.load`. To save, it writes the bytes of [`Document::text`]. The
//! text is kept exactly as loaded, so an unedited document saves back byte
//! for byte.
//!
//! ```
//! use bite_engine::{Engine, command_id};
//!
//! let mut engine = Engine::new();
//! let id = engine.dispatch(command_id::CREATE, &[])?.id().to_string();
//!
//! let edit = [("document", id.as_str()), ("text", "hello")];
//! engine.dispatch(command_id::REPLACE_TEXT, &edit)?;
//! let undone = engine.dispatch(command_id::UNDO, &[("document", id.as_str())])?;
//!
//! let document = undone.document().expect("undo leaves the document open");
//! assert_eq!(document.text(), "");
//! assert!(document.can_redo());
//! # Ok::<(), bite_engine::EngineError>(())
//! ```

use std::collections::BTreeMap;
use std::fmt;
use std::num::ParseIntError;
use std::ops::Range;
use std::str::FromStr;

/// The stable string IDs commands are dispatched under.
pub mod command_id {
    pub const CREATE: &str = "document.create";
    pub const LOAD: &str = "document.load";
    pub const REPLACE_TEXT: &str = "document.replace_text";
    pub const REPLACE_RANGE: &str = "document.replace_range";
    pub const UNDO: &str = "document.undo";
    pub const REDO: &str = "document.redo";
    pub const CLOSE: &str = "document.close";
}

/// Describes one command: its stable ID and the parameters it requires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandSpec {
    pub id: &'static str,
    pub summary: &'static str,
    pub params: &'static [&'static str],
}

/// Every command the engine understands.
pub const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: command_id::CREATE,
        summary: "Creates an empty document.",
        params: &[],
    },
    CommandSpec {
        id: command_id::LOAD,
        summary: "Creates a document holding the given text, with no undo history.",
        params: &["text"],
    },
    CommandSpec {
        id: command_id::REPLACE_TEXT,
        summary: "Replaces the whole text of a document.",
        params: &["document", "text"],
    },
    CommandSpec {
        id: command_id::REPLACE_RANGE,
        summary: "Replaces the bytes from offset `start` up to but not including `end` with the given text.",
        params: &["document", "start", "end", "text"],
    },
    CommandSpec {
        id: command_id::UNDO,
        summary: "Reverts the most recent undo step of a document.",
        params: &["document"],
    },
    CommandSpec {
        id: command_id::REDO,
        summary: "Reapplies the most recently undone step of a document.",
        params: &["document"],
    },
    CommandSpec {
        id: command_id::CLOSE,
        summary: "Removes a document and its history from the engine.",
        params: &["document"],
    },
];

/// A validated request to change engine state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    CreateDocument,
    LoadDocument {
        text: String,
    },
    ReplaceText {
        document: DocumentId,
        text: String,
    },
    /// Replaces the bytes `start..end` of the text, as UTF-8 byte offsets.
    ReplaceRange {
        document: DocumentId,
        start: usize,
        end: usize,
        text: String,
    },
    Undo {
        document: DocumentId,
    },
    Redo {
        document: DocumentId,
    },
    Close {
        document: DocumentId,
    },
}

impl Command {
    /// Builds a command from untrusted input: a command ID from [`COMMANDS`]
    /// and that command's named parameters, in any order.
    pub fn parse(id: &str, params: &[(&str, &str)]) -> Result<Self, EngineError> {
        let unknown_command = || EngineError::UnknownCommand(id.to_owned());
        let spec = COMMANDS
            .iter()
            .find(|spec| spec.id == id)
            .ok_or_else(unknown_command)?;
        let args = Args::new(spec, params)?;
        match spec.id {
            command_id::CREATE => Ok(Self::CreateDocument),
            command_id::LOAD => Ok(Self::LoadDocument {
                text: args.get("text")?.to_owned(),
            }),
            command_id::REPLACE_TEXT => Ok(Self::ReplaceText {
                document: args.document()?,
                text: args.get("text")?.to_owned(),
            }),
            command_id::REPLACE_RANGE => Ok(Self::ReplaceRange {
                document: args.document()?,
                start: args.offset("start")?,
                end: args.offset("end")?,
                text: args.get("text")?.to_owned(),
            }),
            command_id::UNDO => Ok(Self::Undo {
                document: args.document()?,
            }),
            command_id::REDO => Ok(Self::Redo {
                document: args.document()?,
            }),
            command_id::CLOSE => Ok(Self::Close {
                document: args.document()?,
            }),
            _ => Err(unknown_command()),
        }
    }
}

/// The named parameters given to one command.
struct Args<'a> {
    command: &'static str,
    params: &'a [(&'a str, &'a str)],
}

impl<'a> Args<'a> {
    /// Rejects parameters the command does not take and parameters given twice.
    fn new(spec: &CommandSpec, params: &'a [(&'a str, &'a str)]) -> Result<Self, EngineError> {
        let command = spec.id;
        for (index, (name, _)) in params.iter().enumerate() {
            let name = *name;
            if !spec.params.contains(&name) {
                let name = name.to_owned();
                return Err(EngineError::UnknownParameter { command, name });
            }
            if params
                .iter()
                .take(index)
                .any(|(earlier, _)| *earlier == name)
            {
                let name = name.to_owned();
                return Err(EngineError::DuplicateParameter { command, name });
            }
        }
        Ok(Self { command, params })
    }

    fn get(&self, name: &'static str) -> Result<&'a str, EngineError> {
        let command = self.command;
        self.params
            .iter()
            .find(|(given, _)| *given == name)
            .map(|(_, value)| *value)
            .ok_or(EngineError::MissingParameter { command, name })
    }

    fn document(&self) -> Result<DocumentId, EngineError> {
        let value = self.get("document")?;
        value.parse().map_err(|_| EngineError::InvalidDocumentId {
            command: self.command,
            value: value.to_owned(),
        })
    }

    /// Reads a byte offset written as ASCII decimal digits, with no sign.
    fn offset(&self, name: &'static str) -> Result<usize, EngineError> {
        let value = self.get(name)?;
        let invalid = || EngineError::InvalidOffset {
            command: self.command,
            name,
            value: value.to_owned(),
        };
        // `usize::from_str` also accepts a leading `+`.
        if !value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(invalid());
        }
        value.parse().map_err(|_| invalid())
    }
}

/// Identifies a document within one [`Engine`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DocumentId(u64);

impl fmt::Display for DocumentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for DocumentId {
    type Err = ParseIntError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value.parse().map(Self)
    }
}

impl From<DocumentId> for u64 {
    fn from(id: DocumentId) -> Self {
        id.0
    }
}

/// An in-memory plain-text document and its undo/redo history.
///
/// History is kept as whole-text snapshots, one per undo step. Equality also
/// compares whether the next range edit would join the current undo step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    id: DocumentId,
    text: String,
    undo_stack: Vec<String>,
    redo_stack: Vec<String>,
    open_step: Option<OpenStep>,
}

/// How the next range edit can join the undo step the previous one recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpenStep {
    /// Text was inserted or replaced; an insertion at this offset joins.
    Typing { at: usize },
    /// Text was deleted from this offset; a deletion that ends or starts here
    /// joins.
    Deleting { at: usize },
}

impl Document {
    fn new(id: DocumentId, text: String) -> Self {
        Self {
            id,
            text,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            open_step: None,
        }
    }

    pub fn id(&self) -> DocumentId {
        self.id
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Replacing the text with identical text is not a change: it records no
    /// undo step and keeps the redo history. Returns the changed range.
    fn replace_text(&mut self, text: String) -> Option<Range<usize>> {
        if text == self.text {
            return None;
        }
        let previous = std::mem::replace(&mut self.text, text);
        let changed = changed_range(&previous, &self.text);
        self.undo_stack.push(previous);
        self.redo_stack.clear();
        self.open_step = None;
        changed
    }

    /// Replaces the bytes `start..end`. An insertion that continues typing,
    /// or a deletion that continues Backspace or Forward Delete, joins the
    /// previous range edit's undo step; anything else starts a new one.
    /// Replacing a range with identical text is not a change. Returns the
    /// changed range, found by comparing only the replaced bytes.
    fn replace_range(
        &mut self,
        start: usize,
        end: usize,
        text: String,
    ) -> Result<Option<Range<usize>>, EngineError> {
        self.check_range(start, end)?;
        let Some(changed) = changed_range(&self.text[start..end], &text) else {
            return Ok(None);
        };
        let inserts = !text.is_empty();
        let deletes = start < end;
        let joins = match (self.open_step, deletes, inserts) {
            (Some(OpenStep::Typing { at }), false, true) => start == at,
            (Some(OpenStep::Deleting { at }), true, false) => end == at || start == at,
            _ => false,
        };
        if !joins {
            self.undo_stack.push(self.text.clone());
        }
        self.text.replace_range(start..end, &text);
        self.redo_stack.clear();
        self.open_step = Some(if inserts {
            OpenStep::Typing {
                at: start + text.len(),
            }
        } else {
            OpenStep::Deleting { at: start }
        });
        Ok(Some(start + changed.start..start + changed.end))
    }

    fn check_range(&self, start: usize, end: usize) -> Result<(), EngineError> {
        let document = self.id;
        if start > end {
            return Err(EngineError::StartAfterEnd {
                document,
                start,
                end,
            });
        }
        let len = self.text.len();
        for (name, offset) in [("start", start), ("end", end)] {
            if offset > len {
                return Err(EngineError::OffsetPastEnd {
                    document,
                    name,
                    offset,
                    len,
                });
            }
            if !self.text.is_char_boundary(offset) {
                return Err(EngineError::OffsetInsideCharacter {
                    document,
                    name,
                    offset,
                });
            }
        }
        Ok(())
    }

    /// Returns the changed range.
    fn undo(&mut self) -> Result<Option<Range<usize>>, EngineError> {
        let previous = self
            .undo_stack
            .pop()
            .ok_or(EngineError::NothingToUndo(self.id))?;
        let undone = std::mem::replace(&mut self.text, previous);
        let changed = changed_range(&undone, &self.text);
        self.redo_stack.push(undone);
        self.open_step = None;
        Ok(changed)
    }

    /// Returns the changed range.
    fn redo(&mut self) -> Result<Option<Range<usize>>, EngineError> {
        let next = self
            .redo_stack
            .pop()
            .ok_or(EngineError::NothingToRedo(self.id))?;
        let current = std::mem::replace(&mut self.text, next);
        let changed = changed_range(&current, &self.text);
        self.undo_stack.push(current);
        self.open_step = None;
        Ok(changed)
    }
}

/// Why a command was rejected. A rejected command changes nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineError {
    UnknownCommand(String),
    UnknownParameter {
        command: &'static str,
        name: String,
    },
    DuplicateParameter {
        command: &'static str,
        name: String,
    },
    MissingParameter {
        command: &'static str,
        name: &'static str,
    },
    InvalidDocumentId {
        command: &'static str,
        value: String,
    },
    /// An offset parameter that is not written as ASCII decimal digits, or
    /// is too large to be an offset.
    InvalidOffset {
        command: &'static str,
        name: &'static str,
        value: String,
    },
    StartAfterEnd {
        document: DocumentId,
        start: usize,
        end: usize,
    },
    /// The offset named `name` is past the text, which is `len` bytes long.
    OffsetPastEnd {
        document: DocumentId,
        name: &'static str,
        offset: usize,
        len: usize,
    },
    /// The offset named `name` falls inside the UTF-8 bytes of one character.
    OffsetInsideCharacter {
        document: DocumentId,
        name: &'static str,
        offset: usize,
    },
    UnknownDocument(DocumentId),
    NothingToUndo(DocumentId),
    NothingToRedo(DocumentId),
    /// Bytes offered as text stop being UTF-8 at `offset`.
    InvalidUtf8 {
        offset: usize,
    },
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownCommand(id) => write!(f, "unknown command {id:?}"),
            Self::UnknownParameter { command, name } => {
                write!(
                    f,
                    "command `{command}` does not take a parameter named {name:?}"
                )
            }
            Self::DuplicateParameter { command, name } => {
                write!(
                    f,
                    "command `{command}` was given parameter {name:?} more than once"
                )
            }
            Self::MissingParameter { command, name } => {
                write!(f, "command `{command}` requires parameter `{name}`")
            }
            Self::InvalidDocumentId { command, value } => {
                write!(
                    f,
                    "command `{command}`: {value:?} is not a valid document ID"
                )
            }
            Self::InvalidOffset {
                command,
                name,
                value,
            } => {
                write!(
                    f,
                    "command `{command}`: {value:?} is not a valid byte offset for `{name}`"
                )
            }
            Self::StartAfterEnd {
                document,
                start,
                end,
            } => {
                write!(
                    f,
                    "document {document}: `start` {start} is after `end` {end}"
                )
            }
            Self::OffsetPastEnd {
                document,
                name,
                offset,
                len,
            } => {
                write!(
                    f,
                    "document {document}: `{name}` {offset} is past the end of the text, \
                     which is {len} bytes long"
                )
            }
            Self::OffsetInsideCharacter {
                document,
                name,
                offset,
            } => {
                write!(
                    f,
                    "document {document}: `{name}` {offset} falls inside a character"
                )
            }
            Self::UnknownDocument(id) => write!(f, "no document with ID {id}"),
            Self::NothingToUndo(id) => write!(f, "document {id} has nothing to undo"),
            Self::NothingToRedo(id) => write!(f, "document {id} has nothing to redo"),
            Self::InvalidUtf8 { offset } => {
                write!(f, "not valid UTF-8 text: invalid byte at offset {offset}")
            }
        }
    }
}

impl std::error::Error for EngineError {}

/// Reads file contents as text for [`command_id::LOAD`].
///
/// Only UTF-8 is accepted. Nothing is normalized: line endings and a leading
/// byte-order mark stay in the text, so the bytes of [`Document::text`] are
/// what a host writes back when saving.
pub fn text_from_utf8(bytes: &[u8]) -> Result<&str, EngineError> {
    std::str::from_utf8(bytes).map_err(|error| EngineError::InvalidUtf8 {
        offset: error.valid_up_to(),
    })
}

/// What a command left behind: the document it created or changed, or the
/// ID of the document it closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome<'a> {
    Document {
        document: &'a Document,
        /// The byte range of the new text that differs from the text
        /// before the command, or `None` if the text did not change.
        changed: Option<Range<usize>>,
    },
    Closed(DocumentId),
}

impl<'a> Outcome<'a> {
    /// The document, unless the command closed it.
    pub fn document(&self) -> Option<&'a Document> {
        match self {
            Self::Document { document, .. } => Some(document),
            Self::Closed(_) => None,
        }
    }

    pub fn id(&self) -> DocumentId {
        match self {
            Self::Document { document, .. } => document.id(),
            Self::Closed(id) => *id,
        }
    }

    /// The byte range of the new text that the command changed. `None` when
    /// the text did not change or the document was closed.
    pub fn changed(&self) -> Option<Range<usize>> {
        match self {
            Self::Document { changed, .. } => changed.clone(),
            Self::Closed(_) => None,
        }
    }
}

/// The byte range of `new` that differs from `old`, or `None` if they are
/// equal. The bytes the texts share at the start are left out, moved back to
/// a boundary between Unicode scalar values; then the bytes they share at
/// the end of what remains, moved forward to a boundary. Because the
/// left-out bytes are shared, those boundaries are the same in both texts.
fn changed_range(old: &str, new: &str) -> Option<Range<usize>> {
    if old == new {
        return None;
    }
    let (old, new_bytes) = (old.as_bytes(), new.as_bytes());
    let mut prefix = old
        .iter()
        .zip(new_bytes)
        .take_while(|(a, b)| a == b)
        .count();
    while !new.is_char_boundary(prefix) {
        prefix -= 1;
    }
    let mut suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new_bytes[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    while !new.is_char_boundary(new.len() - suffix) {
        suffix -= 1;
    }
    Some(prefix..new.len() - suffix)
}

/// Owns every open document and applies commands to them.
#[derive(Debug, Default)]
pub struct Engine {
    documents: BTreeMap<DocumentId, Document>,
    last_id: u64,
}

impl Engine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Runs the command registered under `id` with the given named parameters.
    pub fn dispatch(
        &mut self,
        id: &str,
        params: &[(&str, &str)],
    ) -> Result<Outcome<'_>, EngineError> {
        self.execute(Command::parse(id, params)?)
    }

    /// Applies `command` and returns the document it affected, or the ID of
    /// the document it closed.
    pub fn execute(&mut self, command: Command) -> Result<Outcome<'_>, EngineError> {
        let (document, changed) = match command {
            Command::CreateDocument => self.add_document(String::new()),
            Command::LoadDocument { text } => self.add_document(text),
            Command::ReplaceText { document, text } => {
                let document = self.document_mut(document)?;
                let changed = document.replace_text(text);
                (document, changed)
            }
            Command::ReplaceRange {
                document,
                start,
                end,
                text,
            } => {
                let document = self.document_mut(document)?;
                let changed = document.replace_range(start, end, text)?;
                (document, changed)
            }
            Command::Undo { document } => {
                let document = self.document_mut(document)?;
                let changed = document.undo()?;
                (document, changed)
            }
            Command::Redo { document } => {
                let document = self.document_mut(document)?;
                let changed = document.redo()?;
                (document, changed)
            }
            Command::Close { document } => {
                self.documents
                    .remove(&document)
                    .ok_or(EngineError::UnknownDocument(document))?;
                return Ok(Outcome::Closed(document));
            }
        };
        Ok(Outcome::Document { document, changed })
    }

    pub fn document(&self, id: DocumentId) -> Option<&Document> {
        self.documents.get(&id)
    }

    /// A new document reports its whole text as changed.
    fn add_document(&mut self, text: String) -> (&mut Document, Option<Range<usize>>) {
        self.last_id += 1;
        let id = DocumentId(self.last_id);
        let changed = Some(0..text.len());
        let document = self.documents.entry(id).or_insert(Document::new(id, text));
        (document, changed)
    }

    fn document_mut(&mut self, id: DocumentId) -> Result<&mut Document, EngineError> {
        self.documents
            .get_mut(&id)
            .ok_or(EngineError::UnknownDocument(id))
    }
}
