//! The CLI, run as a separate process the way a person or script runs it.

use bite_engine::COMMANDS;
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// An empty directory for one test, under Cargo's scratch space.
fn scratch(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("cli")
        .join(test);
    if dir.exists() {
        make_writable(&dir);
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn make_writable(dir: &Path) {
    fs::set_permissions(dir, fs::Permissions::from_mode(0o755)).unwrap();
}

fn bite(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_bite-cli"))
        .args(args)
        .output()
        .unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

fn path_arg(path: &Path) -> &str {
    path.to_str().unwrap()
}

/// The names in `dir`, sorted, so a stray temporary file shows up.
fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

#[test]
fn a_missing_file_is_created_empty() {
    let dir = scratch("missing_file_created");
    let file = dir.join("new.txt");

    let output = bite(&[path_arg(&file)]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(fs::read(&file).unwrap(), b"");
    assert_eq!(names(&dir), ["new.txt"]);
}

#[test]
fn a_command_on_a_missing_file_creates_it_with_the_result() {
    let dir = scratch("missing_file_edited");
    let file = dir.join("new.txt");

    let output = bite(&[
        path_arg(&file),
        "document.replace_text",
        "text=first line\n",
    ]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(fs::read(&file).unwrap(), b"first line\n");
}

#[test]
fn commands_are_applied_to_an_existing_file_and_saved() {
    let dir = scratch("existing_file_edited");
    let file = dir.join("notes.txt");
    fs::write(&file, "old text\n").unwrap();

    let output = bite(&[
        path_arg(&file),
        "document.replace_text",
        "text=key = value\r\n",
    ]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(output.stdout.is_empty());
    assert_eq!(fs::read(&file).unwrap(), b"key = value\r\n");
    assert_eq!(names(&dir), ["notes.txt"]);
}

#[test]
fn commands_run_in_order_and_undo_reaches_back_within_the_run() {
    let dir = scratch("commands_in_order");
    let file = dir.join("notes.txt");
    fs::write(&file, "start").unwrap();

    let output = bite(&[
        path_arg(&file),
        "document.replace_text",
        "text=one",
        "document.replace_text",
        "text=two",
        "document.undo",
    ]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(fs::read(&file).unwrap(), b"one");
}

#[test]
fn a_file_that_ends_unchanged_is_left_untouched() {
    let dir = scratch("unchanged_file");
    let file = dir.join("notes.txt");
    let bytes = b"\xEF\xBB\xBFmixed\r\nline\nendings\r";
    fs::write(&file, bytes).unwrap();
    let inode = fs::metadata(&file).unwrap().ino();

    for args in [
        vec![path_arg(&file)],
        vec![
            path_arg(&file),
            "document.replace_text",
            "text=edited",
            "document.undo",
        ],
    ] {
        let output = bite(&args);

        assert!(output.status.success(), "{}", stderr(&output));
        assert_eq!(fs::read(&file).unwrap(), bytes);
        assert_eq!(fs::metadata(&file).unwrap().ino(), inode, "{args:?}");
    }
}

#[test]
fn a_failed_command_saves_nothing() {
    let dir = scratch("failed_command");
    let file = dir.join("notes.txt");
    fs::write(&file, "keep me").unwrap();

    let output = bite(&[
        path_arg(&file),
        "document.replace_text",
        "text=lost",
        "document.undo",
        "document.undo",
    ]);

    assert_eq!(output.status.code(), Some(1));
    let message = stderr(&output);
    assert!(message.starts_with("bite-cli: "), "{message}");
    assert!(message.contains("document.undo"), "{message}");
    assert!(message.contains("nothing to undo"), "{message}");
    assert!(message.contains("nothing was saved"), "{message}");
    assert_eq!(fs::read(&file).unwrap(), b"keep me");
    assert_eq!(names(&dir), ["notes.txt"]);
}

#[test]
fn a_failed_command_does_not_create_a_missing_file() {
    let dir = scratch("failed_command_missing_file");
    let file = dir.join("new.txt");

    let output = bite(&[path_arg(&file), "document.redo"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(names(&dir).is_empty());
}

#[test]
fn engine_errors_are_reported_as_the_engine_words_them() {
    let dir = scratch("engine_errors");
    let file = dir.join("notes.txt");
    fs::write(&file, "keep me").unwrap();

    for (command, expected) in [
        (
            vec!["document.frobnicate"],
            "unknown command \"document.frobnicate\"",
        ),
        (vec!["document.replace_text"], "requires parameter `text`"),
        (
            vec!["document.undo", "text=x"],
            "does not take a parameter named \"text\"",
        ),
    ] {
        let mut args = vec![path_arg(&file)];
        args.extend(command);
        let output = bite(&args);

        assert_eq!(output.status.code(), Some(1), "{args:?}");
        let message = stderr(&output);
        assert!(message.contains(expected), "{message}");
        assert_eq!(fs::read(&file).unwrap(), b"keep me");
    }
}

#[test]
fn a_file_that_is_not_utf8_is_refused_and_left_alone() {
    let dir = scratch("not_utf8");
    let file = dir.join("latin1.txt");
    fs::write(&file, b"caf\xE9\n").unwrap();

    let output = bite(&[path_arg(&file), "document.replace_text", "text=x"]);

    assert_eq!(output.status.code(), Some(1));
    let message = stderr(&output);
    assert!(message.contains("latin1.txt"), "{message}");
    assert!(message.contains("offset 3"), "{message}");
    assert_eq!(fs::read(&file).unwrap(), b"caf\xE9\n");
}

#[test]
fn a_directory_is_not_opened() {
    let dir = scratch("directory");

    let output = bite(&[path_arg(&dir), "document.replace_text", "text=x"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("cannot read"),
        "{}",
        stderr(&output)
    );
    assert!(dir.is_dir());
}

#[test]
fn a_failed_save_keeps_the_file_and_leaves_no_temporary_file() {
    let dir = scratch("failed_save");
    let file = dir.join("notes.txt");
    fs::write(&file, "keep me").unwrap();
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o555)).unwrap();

    let output = bite(&[path_arg(&file), "document.replace_text", "text=lost"]);
    make_writable(&dir);

    assert_eq!(output.status.code(), Some(1));
    let message = stderr(&output);
    assert!(message.contains("cannot save"), "{message}");
    assert!(message.contains("nothing was saved"), "{message}");
    assert_eq!(fs::read(&file).unwrap(), b"keep me");
    assert_eq!(names(&dir), ["notes.txt"]);
}

#[test]
fn a_read_only_file_is_not_replaced() {
    let dir = scratch("read_only_file");
    let file = dir.join("notes.txt");
    fs::write(&file, "keep me").unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o444)).unwrap();

    let output = bite(&[path_arg(&file), "document.replace_text", "text=lost"]);

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(fs::read(&file).unwrap(), b"keep me");
    assert_eq!(names(&dir), ["notes.txt"]);
}

#[test]
fn saving_keeps_the_file_permissions() {
    let dir = scratch("permissions");
    let file = dir.join("private.txt");
    fs::write(&file, "old").unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();

    let output = bite(&[path_arg(&file), "document.replace_text", "text=new"]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(fs::read(&file).unwrap(), b"new");
    assert_eq!(fs::metadata(&file).unwrap().mode() & 0o777, 0o600);
}

#[test]
fn saving_through_a_symbolic_link_updates_the_linked_file() {
    let dir = scratch("symlink");
    let target = dir.join("real.txt");
    let link = dir.join("link.txt");
    fs::write(&target, "old").unwrap();
    symlink("real.txt", &link).unwrap();

    let output = bite(&[path_arg(&link), "document.replace_text", "text=new"]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(fs::symlink_metadata(&link).unwrap().is_symlink());
    assert_eq!(fs::read(&target).unwrap(), b"new");
    assert_eq!(names(&dir), ["link.txt", "real.txt"]);
}

#[test]
fn a_malformed_command_line_is_a_usage_error_and_touches_nothing() {
    let dir = scratch("usage_errors");
    let file = dir.join("new.txt");
    let file = path_arg(&file);

    for args in [
        vec![],
        vec!["-x"],
        vec![file, "text=before any command"],
        vec![file, "document.replace_text", "document=1", "text=x"],
        vec![file, "document.create"],
        vec![file, "document.load", "text=x"],
    ] {
        let output = bite(&args);

        assert_eq!(output.status.code(), Some(2), "{args:?}");
        let message = stderr(&output);
        assert!(message.starts_with("bite-cli: "), "{message}");
        assert!(message.contains("Usage: bite-cli"), "{message}");
        assert!(names(&dir).is_empty(), "{args:?}");
    }
}

#[test]
fn help_lists_every_command_that_acts_on_a_document() {
    for flag in ["-h", "--help"] {
        let output = bite(&[flag]);

        assert!(output.status.success());
        let help = String::from_utf8(output.stdout).unwrap();
        assert!(help.starts_with("Usage: bite-cli"), "{help}");
        for spec in COMMANDS {
            let listed = help.contains(spec.id);
            assert_eq!(listed, spec.params.contains(&"document"), "{}", spec.id);
        }
        assert!(help.contains("text=VALUE"), "{help}");
    }
}
