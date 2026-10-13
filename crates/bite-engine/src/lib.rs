//! Headless, in-memory plain-text document engine for Bite Apps.
//!
//! Every change to a document goes through a command. [`Engine::dispatch`]
//! takes a command ID and named string parameters, which suits untrusted
//! callers such as a CLI; [`Engine::execute`] takes an already typed
//! [`Command`]. Both run the same code, and both return the affected
//! [`Document`] or an [`EngineError`]. No input makes the engine panic.
//!
//! # Commands
//!
//! Command IDs are stable: an ID keeps its meaning once it is listed here.
//!
//! | ID | Parameters | Effect |
//! |---|---|---|
//! | `document.create` | none | Creates an empty document. |
//! | `document.replace_text` | `document`, `text` | Replaces the whole text. |
//! | `document.undo` | `document` | Reverts the most recent change. |
//! | `document.redo` | `document` | Reapplies the most recently undone change. |
//!
//! `document` is the decimal ID of a document created by this engine. Every
//! listed parameter is required, and a command accepts no others.
//!
//! ```
//! use bite_engine::{Engine, command_id};
//!
//! let mut engine = Engine::new();
//! let id = engine.dispatch(command_id::CREATE, &[])?.id().to_string();
//!
//! let edit = [("document", id.as_str()), ("text", "hello")];
//! engine.dispatch(command_id::REPLACE_TEXT, &edit)?;
//! let document = engine.dispatch(command_id::UNDO, &[("document", id.as_str())])?;
//!
//! assert_eq!(document.text(), "");
//! assert!(document.can_redo());
//! # Ok::<(), bite_engine::EngineError>(())
//! ```

use std::collections::BTreeMap;
use std::fmt;
use std::num::ParseIntError;
use std::str::FromStr;

/// The stable string IDs commands are dispatched under.
pub mod command_id {
    pub const CREATE: &str = "document.create";
    pub const REPLACE_TEXT: &str = "document.replace_text";
    pub const UNDO: &str = "document.undo";
    pub const REDO: &str = "document.redo";
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
        id: command_id::REPLACE_TEXT,
        summary: "Replaces the whole text of a document.",
        params: &["document", "text"],
    },
    CommandSpec {
        id: command_id::UNDO,
        summary: "Reverts the most recent change to a document.",
        params: &["document"],
    },
    CommandSpec {
        id: command_id::REDO,
        summary: "Reapplies the most recently undone change to a document.",
        params: &["document"],
    },
];

/// A validated request to change engine state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    CreateDocument,
    ReplaceText { document: DocumentId, text: String },
    Undo { document: DocumentId },
    Redo { document: DocumentId },
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
            command_id::REPLACE_TEXT => Ok(Self::ReplaceText {
                document: args.document()?,
                text: args.get("text")?.to_owned(),
            }),
            command_id::UNDO => Ok(Self::Undo {
                document: args.document()?,
            }),
            command_id::REDO => Ok(Self::Redo {
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
/// History is kept as whole-text snapshots, one per change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    id: DocumentId,
    text: String,
    undo_stack: Vec<String>,
    redo_stack: Vec<String>,
}

impl Document {
    fn new(id: DocumentId) -> Self {
        Self {
            id,
            text: String::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
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
    /// undo step and keeps the redo history.
    fn replace_text(&mut self, text: String) {
        if text == self.text {
            return;
        }
        let previous = std::mem::replace(&mut self.text, text);
        self.undo_stack.push(previous);
        self.redo_stack.clear();
    }

    fn undo(&mut self) -> Result<(), EngineError> {
        let previous = self
            .undo_stack
            .pop()
            .ok_or(EngineError::NothingToUndo(self.id))?;
        let undone = std::mem::replace(&mut self.text, previous);
        self.redo_stack.push(undone);
        Ok(())
    }

    fn redo(&mut self) -> Result<(), EngineError> {
        let next = self
            .redo_stack
            .pop()
            .ok_or(EngineError::NothingToRedo(self.id))?;
        let current = std::mem::replace(&mut self.text, next);
        self.undo_stack.push(current);
        Ok(())
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
    UnknownDocument(DocumentId),
    NothingToUndo(DocumentId),
    NothingToRedo(DocumentId),
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
            Self::UnknownDocument(id) => write!(f, "no document with ID {id}"),
            Self::NothingToUndo(id) => write!(f, "document {id} has nothing to undo"),
            Self::NothingToRedo(id) => write!(f, "document {id} has nothing to redo"),
        }
    }
}

impl std::error::Error for EngineError {}

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
    ) -> Result<&Document, EngineError> {
        self.execute(Command::parse(id, params)?)
    }

    /// Applies `command` and returns the document it affected.
    pub fn execute(&mut self, command: Command) -> Result<&Document, EngineError> {
        let document = match command {
            Command::CreateDocument => {
                self.last_id += 1;
                let id = DocumentId(self.last_id);
                self.documents.entry(id).or_insert(Document::new(id))
            }
            Command::ReplaceText { document, text } => {
                let document = self.document_mut(document)?;
                document.replace_text(text);
                document
            }
            Command::Undo { document } => {
                let document = self.document_mut(document)?;
                document.undo()?;
                document
            }
            Command::Redo { document } => {
                let document = self.document_mut(document)?;
                document.redo()?;
                document
            }
        };
        Ok(document)
    }

    pub fn document(&self, id: DocumentId) -> Option<&Document> {
        self.documents.get(&id)
    }

    fn document_mut(&mut self, id: DocumentId) -> Result<&mut Document, EngineError> {
        self.documents
            .get_mut(&id)
            .ok_or(EngineError::UnknownDocument(id))
    }
}
