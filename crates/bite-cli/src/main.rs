//! Command-line host for the Bite engine.
//!
//! `bite-cli FILE [COMMAND [NAME=VALUE]...]...` opens FILE, runs engine
//! commands on it by their stable IDs, and saves the result. See [`USAGE`].

mod save;

use bite_engine::{COMMANDS, CommandSpec, DocumentId, Engine, command_id, text_from_utf8};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::{env, fs, io};

const USAGE: &str = "\
Usage: bite-cli FILE [COMMAND [NAME=VALUE]...]...

Opens FILE, or starts an empty document if FILE does not exist, runs each
COMMAND on it in order, and saves the result. bite-cli supplies the
`document` parameter. Undo and redo reach back only to changes made earlier
in the same run.

Range offsets count UTF-8 bytes and must not fall inside a character.
Consecutive range edits that continue typing or deleting, as a text editor
would send them, undo as one step.

FILE is saved only if every command succeeds, through a temporary file and a
rename, and is left untouched if its text ends up unchanged. Saving keeps
FILE's mode, group, access control list, and the extended attributes macOS
keeps when it saves a document, such as Finder tags; not its owner, creation
date, flags such as hidden, or the quarantine flag of a download. If its
group cannot be kept, the new group gets no more access than others. Saving
refuses a file with other hard links and a symbolic link to a missing file.
Only UTF-8 files can be opened.

Exit status: 0 on success, 1 if a command or the file fails, 2 for a
malformed command line.

Commands:
";

/// What the command line asks for.
enum Request {
    Help,
    Edit { path: PathBuf, steps: Vec<Step> },
}

/// One command and the parameters given to it on the command line.
struct Step {
    id: String,
    params: Vec<(String, String)>,
}

fn main() -> ExitCode {
    match parse(env::args_os().skip(1)) {
        Ok(Request::Help) => {
            print!("{}", help());
            ExitCode::SUCCESS
        }
        Ok(Request::Edit { path, steps }) => match edit(&path, &steps) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("bite-cli: {message}");
                ExitCode::FAILURE
            }
        },
        Err(message) => {
            eprint!("bite-cli: {message}\n\n{}", help());
            ExitCode::from(2)
        }
    }
}

fn help() -> String {
    let mut help = USAGE.to_owned();
    for spec in COMMANDS.iter().filter(|spec| refusal(spec).is_none()) {
        help.push_str("  ");
        help.push_str(spec.id);
        for param in spec.params.iter().filter(|param| **param != "document") {
            help.push_str(&format!(" {param}=VALUE"));
        }
        help.push_str(&format!("\n      {}\n", spec.summary));
    }
    help
}

fn acts_on_document(spec: &CommandSpec) -> bool {
    spec.params.contains(&"document")
}

/// Why bite-cli cannot run the command, or `None` if it can.
fn refusal(spec: &CommandSpec) -> Option<&'static str> {
    if !acts_on_document(spec) {
        Some("bite-cli opens FILE itself")
    } else if [command_id::CLOSE, command_id::MARK_SAVED].contains(&spec.id) {
        Some("bite-cli saves FILE after the last command")
    } else {
        None
    }
}

fn parse(args: impl Iterator<Item = OsString>) -> Result<Request, String> {
    // A request for help wins over anything else on the command line.
    let args: Vec<OsString> = args.collect();
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        return Ok(Request::Help);
    }
    let mut args = args.into_iter();
    let Some(path) = args.next() else {
        return Err("missing FILE".to_owned());
    };
    if path.as_encoded_bytes().starts_with(b"-") {
        return Err(format!("unknown option {}", path.to_string_lossy()));
    }

    let mut steps: Vec<Step> = Vec::new();
    for arg in args {
        let arg = arg
            .into_string()
            .map_err(|arg| format!("argument {arg:?} is not valid UTF-8"))?;
        let Some((name, value)) = arg.split_once('=') else {
            if arg.starts_with('-') {
                return Err(format!("unknown option {arg}"));
            }
            let spec = COMMANDS.iter().find(|spec| spec.id == arg);
            if let Some(reason) = spec.and_then(refusal) {
                return Err(format!("`{arg}` cannot be used here: {reason}"));
            }
            steps.push(Step {
                id: arg,
                params: Vec::new(),
            });
            continue;
        };
        if name == "document" {
            return Err("bite-cli supplies the `document` parameter itself".to_owned());
        }
        let Some(step) = steps.last_mut() else {
            return Err(format!("parameter {arg:?} comes before any command"));
        };
        step.params.push((name.to_owned(), value.to_owned()));
    }
    Ok(Request::Edit {
        path: PathBuf::from(path),
        steps,
    })
}

/// Opens the file, runs `steps` on it, and saves it if its text changed.
fn edit(path: &Path, steps: &[Step]) -> Result<(), String> {
    let shown = path.display();
    let original = match fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(format!("cannot read {shown}: {error}")),
    };

    let mut engine = Engine::new();
    let opened = match &original {
        Some(bytes) => {
            let text =
                text_from_utf8(bytes).map_err(|error| format!("cannot open {shown}: {error}"))?;
            engine.dispatch(command_id::LOAD, &[("text", text)])
        }
        None => engine.dispatch(command_id::CREATE, &[]),
    };
    let id = opened.map_err(|error| error.to_string())?.id();
    let document = id.to_string();

    for step in steps {
        let mut params: Vec<(&str, &str)> = step
            .params
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect();
        if COMMANDS
            .iter()
            .any(|spec| spec.id == step.id && acts_on_document(spec))
        {
            params.push(("document", &document));
        }
        engine
            .dispatch(&step.id, &params)
            .map_err(|error| format!("{}: {error}; nothing was saved", step.id))?;
    }

    let text = text_of(&engine, id)?;
    if original.as_deref() == Some(text.as_bytes()) {
        return Ok(());
    }
    save::save(path, text.as_bytes())
        .map_err(|error| format!("cannot save {shown}: {error}; nothing was saved"))
}

fn text_of(engine: &Engine, id: DocumentId) -> Result<&str, String> {
    engine
        .document(id)
        .map(|document| document.text())
        .ok_or_else(|| format!("document {id} disappeared from the engine"))
}
