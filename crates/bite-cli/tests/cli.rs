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
        // Access control lists from an earlier run could block the removal.
        let _ = Command::new("chmod").arg("-RN").arg(&dir).status();
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

/// Runs a system tool the test needs and returns its standard output.
fn tool(program: &str, args: &[&str]) -> String {
    let output = Command::new(program).args(args).output().unwrap();
    assert!(output.status.success(), "{program} {args:?}: {output:?}");
    String::from_utf8(output.stdout).unwrap()
}

/// Root ignores permission bits, so tests that rely on them cannot run as root.
fn running_as_root(dir: &Path) -> bool {
    let probe = dir.join("owner-probe");
    fs::write(&probe, "").unwrap();
    let root = fs::metadata(&probe).unwrap().uid() == 0;
    fs::remove_file(&probe).unwrap();
    if root {
        eprintln!("skipped: permission bits do not apply to root");
    }
    root
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
    let inode = fs::metadata(&file).unwrap().ino();

    let output = bite(&[
        path_arg(&file),
        "document.replace_text",
        "text=key = value\r\n",
    ]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(output.stdout.is_empty());
    assert_eq!(fs::read(&file).unwrap(), b"key = value\r\n");
    assert_eq!(names(&dir), ["notes.txt"]);
    // A new inode shows the file was replaced whole, not rewritten in place.
    assert_ne!(fs::metadata(&file).unwrap().ino(), inode);
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
fn range_edits_change_bytes_in_place_and_undo_as_typing_does() {
    let dir = scratch("range_edits");
    let file = dir.join("notes.txt");
    fs::write(&file, "caf\u{e9}\n").unwrap();

    let output = bite(&[
        path_arg(&file),
        "document.replace_range",
        "start=5",
        "end=5",
        "text=!",
        "document.replace_range",
        "start=6",
        "end=6",
        "text=!",
        "document.replace_range",
        "start=0",
        "end=0",
        "text=Le ",
        "document.undo",
        "document.undo",
    ]);

    // The first undo removes "Le "; the second removes both "!", typed as one step.
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(fs::read_to_string(&file).unwrap(), "caf\u{e9}\n");
}

#[test]
fn a_range_edit_inside_a_character_saves_nothing() {
    let dir = scratch("range_edit_inside_character");
    let file = dir.join("notes.txt");
    fs::write(&file, "caf\u{e9}\n").unwrap();

    let output = bite(&[
        path_arg(&file),
        "document.replace_range",
        "start=4",
        "end=4",
        "text=x",
    ]);

    assert_eq!(output.status.code(), Some(1));
    let message = stderr(&output);
    assert!(
        message.contains("`start` 4 falls inside a character"),
        "{message}"
    );
    assert!(message.contains("nothing was saved"), "{message}");
    assert_eq!(fs::read_to_string(&file).unwrap(), "caf\u{e9}\n");
}

#[test]
fn closing_or_marking_the_document_saved_is_refused_because_bite_cli_saves_it() {
    let dir = scratch("close_refused");
    let file = dir.join("notes.txt");
    fs::write(&file, "kept").unwrap();

    for command in ["document.close", "document.mark_saved"] {
        let output = bite(&[path_arg(&file), command]);

        assert_eq!(output.status.code(), Some(2), "{command}");
        let message = stderr(&output);
        assert!(
            message.contains(&format!("`{command}` cannot be used here")),
            "{message}"
        );
        assert!(message.contains("saves"), "{message}");
        assert_eq!(fs::read(&file).unwrap(), b"kept");
    }
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
    if running_as_root(&dir) {
        return;
    }
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
    if running_as_root(&dir) {
        return;
    }
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
fn a_symbolic_link_to_a_missing_file_is_not_replaced() {
    let dir = scratch("dangling_symlink");
    let link = dir.join("dangling.txt");
    symlink("missing.txt", &link).unwrap();

    let output = bite(&[path_arg(&link), "document.replace_text", "text=lost"]);

    assert_eq!(output.status.code(), Some(1));
    let message = stderr(&output);
    assert!(message.contains("symbolic link"), "{message}");
    assert!(message.contains("nothing was saved"), "{message}");
    assert_eq!(fs::read_link(&link).unwrap(), Path::new("missing.txt"));
    assert_eq!(names(&dir), ["dangling.txt"]);
}

#[test]
fn a_file_with_other_hard_links_is_not_replaced() {
    let dir = scratch("hard_link");
    let file = dir.join("a.txt");
    let other = dir.join("b.txt");
    fs::write(&file, "keep me").unwrap();
    fs::hard_link(&file, &other).unwrap();

    let output = bite(&[path_arg(&file), "document.replace_text", "text=lost"]);

    assert_eq!(output.status.code(), Some(1));
    let message = stderr(&output);
    assert!(message.contains("hard links"), "{message}");
    assert!(message.contains("nothing was saved"), "{message}");
    assert_eq!(fs::read(&file).unwrap(), b"keep me");
    assert_eq!(fs::metadata(&file).unwrap().nlink(), 2);
    assert_eq!(names(&dir), ["a.txt", "b.txt"]);
}

#[test]
#[cfg(target_os = "macos")]
fn saving_keeps_finder_tags_other_extended_attributes_and_the_acl() {
    let dir = scratch("attributes");
    let file = dir.join("tagged.txt");
    let path = path_arg(&file);
    fs::write(&file, "old").unwrap();
    let tags = "com.apple.metadata:_kMDItemUserTags";
    tool("xattr", &["-w", tags, "(Red)", path]);
    tool("xattr", &["-w", "com.example.note", "keep me", path]);
    tool("chmod", &["+a", "everyone allow read", path]);

    let output = bite(&[path, "document.replace_text", "text=new"]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(fs::read(&file).unwrap(), b"new");
    assert_eq!(tool("xattr", &["-p", tags, path]), "(Red)\n");
    assert_eq!(
        tool("xattr", &["-p", "com.example.note", path]),
        "keep me\n"
    );
    let listing = tool("ls", &["-le", path]);
    assert!(listing.contains("everyone allow read"), "{listing}");
    assert_eq!(names(&dir), ["tagged.txt"]);
}

#[test]
fn a_malformed_command_line_is_a_usage_error_and_touches_nothing() {
    let dir = scratch("usage_errors");
    let file = dir.join("new.txt");
    let file = path_arg(&file);

    for args in [
        vec![],
        vec!["-x"],
        vec![file, "--frobnicate"],
        vec![file, "document.undo", "-x"],
        vec![file, "text=before any command"],
        vec![file, "document.replace_text", "document=1", "text=x"],
        vec![file, "document.create"],
        vec![file, "document.load", "text=x"],
        vec![file, "document.close"],
        vec![file, "document.replace_text", "text=x", "document.close"],
        vec![file, "document.mark_saved"],
        vec![
            file,
            "document.replace_text",
            "text=x",
            "document.mark_saved",
        ],
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
fn help_lists_every_command_that_edits_a_document() {
    for flag in ["-h", "--help"] {
        let output = bite(&[flag]);

        assert!(output.status.success());
        let help = String::from_utf8(output.stdout).unwrap();
        assert!(help.starts_with("Usage: bite-cli"), "{help}");
        for spec in COMMANDS {
            // bite-cli saves the document after the last command, so it
            // cannot close it or mark it saved.
            let usable = spec.params.contains(&"document")
                && !["document.close", "document.mark_saved"].contains(&spec.id);
            assert_eq!(help.contains(spec.id), usable, "{}", spec.id);
        }
        assert!(help.contains("text=VALUE"), "{help}");
    }
}

#[test]
fn help_anywhere_on_the_command_line_prints_help_and_touches_nothing() {
    let dir = scratch("help_anywhere");
    let file = dir.join("new.txt");
    let file = path_arg(&file);

    for args in [
        vec![file, "--help"],
        vec![file, "document.undo", "--help"],
        vec![file, "text=before any command", "--help"],
        vec![file, "document.create", "-h"],
        vec![file, "document.undo", "document=1", "--help"],
    ] {
        let output = bite(&args);

        assert!(output.status.success(), "{args:?}: {}", stderr(&output));
        let help = String::from_utf8(output.stdout).unwrap();
        assert!(help.starts_with("Usage: bite-cli"), "{help}");
        assert!(names(&dir).is_empty(), "{args:?}");
    }
}

#[test]
fn a_new_file_gets_the_permissions_any_new_file_gets() {
    let dir = scratch("new_file_mode");
    let probe = dir.join("probe.txt");
    fs::write(&probe, "").unwrap();
    let usual = fs::metadata(&probe).unwrap().mode() & 0o777;
    let file = dir.join("new.txt");

    let output = bite(&[path_arg(&file), "document.replace_text", "text=new"]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(fs::metadata(&file).unwrap().mode() & 0o777, usual);
}

#[test]
fn saving_keeps_the_file_group() {
    let dir = scratch("group");
    let file = dir.join("shared.txt");
    fs::write(&file, "old").unwrap();
    let folder_group = fs::metadata(&dir).unwrap().gid();
    let groups = tool("id", &["-G"]);
    let Some(other) = groups
        .split_whitespace()
        .map(|gid| gid.parse::<u32>().unwrap())
        .find(|gid| *gid != folder_group)
    else {
        eprintln!("skipped: the user belongs to no group but the folder's");
        return;
    };
    std::os::unix::fs::chown(&file, None, Some(other)).unwrap();

    let output = bite(&[path_arg(&file), "document.replace_text", "text=new"]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(fs::read(&file).unwrap(), b"new");
    assert_eq!(fs::metadata(&file).unwrap().gid(), other);
}

#[test]
#[cfg(target_os = "macos")]
fn saving_drops_attributes_macos_drops_when_it_saves_a_document() {
    let dir = scratch("dropped_attributes");
    let file = dir.join("notes.txt");
    let path = path_arg(&file);
    fs::write(&file, "plain").unwrap();
    // TextEdit records the encoding it used; it would be stale after a save.
    tool(
        "xattr",
        &["-w", "com.apple.TextEncoding", "macintosh;0", path],
    );
    // A downloaded file's quarantine flag is not kept either.
    tool(
        "xattr",
        &["-w", "com.apple.quarantine", "0081;00000000;Safari;", path],
    );

    let output = bite(&[path, "document.replace_text", "text=caf\u{e9}"]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(fs::read(&file).unwrap(), "caf\u{e9}".as_bytes());
    let attributes = tool("xattr", &[path]);
    assert!(!attributes.contains("TextEncoding"), "{attributes}");
    assert!(!attributes.contains("quarantine"), "{attributes}");
}

#[test]
#[cfg(target_os = "macos")]
fn saving_gives_no_access_the_original_did_not_have() {
    let dir = scratch("inherited_acl");
    let file = dir.join("notes.txt");
    let path = path_arg(&file);
    fs::write(&file, "old").unwrap();
    // Added after the file was created, so only a new file would inherit it.
    tool(
        "chmod",
        &["+a", "user:daemon allow read,file_inherit", path_arg(&dir)],
    );

    let output = bite(&[path, "document.replace_text", "text=new"]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(fs::read(&file).unwrap(), b"new");
    let listing = tool("ls", &["-le", path]);
    assert!(!listing.contains("daemon"), "{listing}");
}

#[test]
#[cfg(target_os = "macos")]
fn a_file_whose_group_cannot_be_kept_gives_its_new_group_no_more_than_others() {
    let dir = scratch("foreign_group");
    // A file made in /tmp gets its group, wheel, and keeps it when moved.
    let made = Path::new("/private/tmp").join(format!("bite-cli-group-{}", std::process::id()));
    fs::write(&made, "old").unwrap();
    let file = dir.join("moved.txt");
    if let Err(error) = fs::rename(&made, &file) {
        fs::remove_file(&made).unwrap();
        eprintln!("skipped: cannot move a file from /tmp here: {error}");
        return;
    }
    fs::set_permissions(&file, fs::Permissions::from_mode(0o664)).unwrap();
    let group = fs::metadata(&file).unwrap().gid();
    let folder_group = fs::metadata(&dir).unwrap().gid();
    let mine = tool("id", &["-G"]);
    if group == folder_group || mine.split_whitespace().any(|gid| gid == group.to_string()) {
        eprintln!("skipped: the user belongs to the group of a file made in /tmp");
        return;
    }

    let output = bite(&[path_arg(&file), "document.replace_text", "text=new"]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(fs::read(&file).unwrap(), b"new");
    let metadata = fs::metadata(&file).unwrap();
    assert_eq!(metadata.gid(), folder_group);
    // Group members lose write, which others never had, and keep read.
    assert_eq!(metadata.mode() & 0o777, 0o644);
}

#[test]
#[cfg(target_os = "macos")]
fn a_save_refused_by_the_access_control_list_leaves_no_temporary_file() {
    let dir = scratch("deny_delete");
    let file = dir.join("guarded.txt");
    let path = path_arg(&file);
    fs::write(&file, "keep me").unwrap();
    tool("chmod", &["+a", "everyone deny delete", path]);

    let output = bite(&[path, "document.replace_text", "text=lost"]);
    let left = names(&dir);
    tool("chmod", &["-RN", path_arg(&dir)]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("nothing was saved"),
        "{}",
        stderr(&output)
    );
    assert_eq!(fs::read(&file).unwrap(), b"keep me");
    assert_eq!(left, ["guarded.txt"]);
}

/// Mounts a new exFAT disk image, which has no access control lists, runs
/// `test` in it, and detaches it.
#[cfg(target_os = "macos")]
fn on_exfat(name: &str, test: impl FnOnce(&Path)) {
    let dir = scratch(name);
    let image = dir.join("volume.dmg");
    let mount = dir.join("mount");
    fs::create_dir(&mount).unwrap();
    tool(
        "hdiutil",
        &[
            "create",
            "-quiet",
            "-size",
            "20m",
            "-fs",
            "ExFAT",
            "-volname",
            "BITE",
            path_arg(&image),
        ],
    );
    tool(
        "hdiutil",
        &[
            "attach",
            "-quiet",
            "-nobrowse",
            "-mountpoint",
            path_arg(&mount),
            path_arg(&image),
        ],
    );
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| test(&mount)));
    tool("hdiutil", &["detach", "-quiet", path_arg(&mount)]);
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}

#[test]
#[cfg(target_os = "macos")]
#[ignore = "creates and mounts a disk image; run with `cargo test -- --ignored`"]
fn saving_on_a_volume_without_access_control_lists_works() {
    on_exfat("exfat", |volume| {
        let file = volume.join("notes.txt");
        let path = path_arg(&file);
        fs::write(&file, "old").unwrap();
        let tags = "com.apple.metadata:_kMDItemUserTags";
        tool("xattr", &["-w", tags, "(Red)", path]);

        let edited = bite(&[path, "document.replace_text", "text=new"]);
        let created = bite(&[
            path_arg(&volume.join("new.txt")),
            "document.replace_text",
            "text=hi",
        ]);

        assert!(edited.status.success(), "{}", stderr(&edited));
        assert!(created.status.success(), "{}", stderr(&created));
        assert_eq!(fs::read(&file).unwrap(), b"new");
        assert_eq!(tool("xattr", &["-p", tags, path]), "(Red)\n");
        let stray: Vec<String> = names(volume)
            .into_iter()
            .filter(|name| name.contains(".bite-"))
            .collect();
        assert!(stray.is_empty(), "{stray:?}");
    });
}
