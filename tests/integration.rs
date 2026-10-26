//! Integration tests: run the `pseudofat` binary, feed commands to stdin, check the output.
//!
//! `OK`, text output (ls, pwd) and parse errors go to stdout, command errors go to stderr.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

// ===== Helpers ======

struct Run {
    stdout: Vec<String>,
    stderr: String,
}

/// Path to a not-yet-existing FS file in cargo's tmp dir (tests run in parallel -> unique name per test)
fn fs_file(name: &str) -> PathBuf {
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}.dat"));
    let _ = std::fs::remove_file(&path);
    path
}

/// Runs the binary over `fs` with `input` as stdin
fn run(fs: &Path, input: &str) -> Run {
    let mut child = Command::new(env!("CARGO_BIN_EXE_pseudofat"))
        .arg(fs)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("cannot start pseudofat");

    // stdin is dropped at the end of the statement -> shell gets EOF and ends
    child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    let output = child.wait_with_output().unwrap();

    Run {
        stdout: String::from_utf8_lossy(&output.stdout).lines().map(str::to_owned).collect(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// New FS (format 1MB) followed by `commands`. The `OK` from format is removed from stdout
fn fresh(name: &str, commands: &str) -> Run {
    let mut run = run(&fs_file(name), &format!("format 1MB\n{commands}"));
    assert_eq!(run.stdout.first().map(String::as_str), Some("OK"), "format failed: {}", run.stderr);
    run.stdout.remove(0);
    run
}

/// First line printed when an existing FS is opened (contains "Free clusters: N")
fn banner(fs: &Path) -> String {
    let run = run(fs, "");
    assert!(run.stdout[0].contains("filesystem loaded"), "{:?}", run.stdout);
    run.stdout[0].clone()
}

// ===== Parsing ======

#[test]
fn unknown_command() {
    let r = run(&fs_file("unknown_command"), "foo\n");
    assert_eq!(r.stdout, ["UNKNOWN COMMAND (foo)"]);
}

#[test]
fn wrong_arguments() {
    let r = run(&fs_file("wrong_arguments"), "mkdir\nls a b\n");
    assert_eq!(r.stdout, ["WRONG ARGUMENTS (usage: mkdir <a1>)", "WRONG ARGUMENTS (usage: ls [a1])"]);
}

#[test]
fn empty_lines_are_ignored() {
    let r = fresh("empty_lines", "\n   \npwd\n");
    assert_eq!(r.stdout, ["/"]);
}

// ===== format ======

#[test]
fn not_formatted() {
    let r = run(&fs_file("not_formatted"), "ls\npwd\n");
    assert!(r.stdout.is_empty(), "{:?}", r.stdout);
    // startup + ls + pwd
    assert_eq!(r.stderr.matches("Disk is not formatted.").count(), 3, "{}", r.stderr);
}

#[test]
fn format_ok() {
    let r = run(&fs_file("format_ok"), "format 1MB\npwd\n");
    assert_eq!(r.stdout, ["OK", "/"]);
}

#[test]
fn format_invalid_size() {
    let r = run(&fs_file("format_invalid_size"), "format abc\n");
    assert!(r.stdout.is_empty(), "{:?}", r.stdout);
    assert!(r.stderr.contains("INVALID SIZE (abc"), "{}", r.stderr);
}

#[test]
fn format_clears_existing_fs() {
    let r = fresh("format_clears", "mkdir a1\nformat 1MB\nls\n");
    assert_eq!(r.stdout, ["OK", "OK"]);
}

#[test]
fn fs_persists_between_runs() {
    let fs = fs_file("persists");
    run(&fs, "format 1MB\nmkdir a1\n");

    let r = run(&fs, "ls\n");
    assert!(r.stdout[0].contains("filesystem loaded"), "{:?}", r.stdout);
    assert_eq!(&r.stdout[1..], &["DIR: a1"]);
}

// ===== exit ======

#[test]
fn exit_stops_processing() {
    let r = fresh("exit", "exit\npwd\n");
    assert!(r.stdout.is_empty(), "{:?}", r.stdout);
}

// ===== pwd, cd ======

#[test]
fn pwd_root() {
    let r = fresh("pwd_root", "pwd\n");
    assert_eq!(r.stdout, ["/"]);
}

#[test]
fn cd_and_pwd() {
    let r = fresh("cd_pwd", "mkdir a1\nmkdir a1/b1\ncd a1/b1\npwd\ncd ..\npwd\ncd /\npwd\ncd ../..\npwd\n");
    assert_eq!(r.stdout, ["OK", "OK", "OK", "/a1/b1", "OK", "/a1", "OK", "/", "OK", "/"]);
}

#[test]
fn cd_not_found_keeps_cwd() {
    let r = fresh("cd_not_found", "mkdir a1\ncd a1\ncd x\npwd\n");
    assert_eq!(r.stdout, ["OK", "OK", "/a1"]);
    assert!(r.stderr.contains("Path not found."), "{}", r.stderr);
}

// ===== ls ======

#[test]
fn ls_empty_dir() {
    let r = fresh("ls_empty", "ls\n");
    assert!(r.stdout.is_empty(), "{:?}", r.stdout);
}

#[test]
fn ls_not_found() {
    let r = fresh("ls_not_found", "ls x\n");
    assert!(r.stdout.is_empty(), "{:?}", r.stdout);
    assert!(r.stderr.contains("Path not found."), "{}", r.stderr);
}

// ===== mkdir ======

#[test]
fn mkdir_and_ls() {
    let r = fresh("mkdir_ls", "mkdir a1\nmkdir a2\nmkdir /a1/b1\nls\nls a1\n");
    assert_eq!(r.stdout, ["OK", "OK", "OK", "DIR: a1", "DIR: a2", "DIR: b1"]);
}

#[test]
fn mkdir_exists() {
    let r = fresh("mkdir_exists", "mkdir a1\nmkdir a1\nmkdir /\nmkdir .\n");
    assert_eq!(r.stdout, ["OK"]);
    assert_eq!(r.stderr.matches("File exists.").count(), 3, "{}", r.stderr);
}

#[test]
fn mkdir_path_not_found() {
    let r = fresh("mkdir_not_found", "mkdir x/y\n");
    assert!(r.stdout.is_empty(), "{:?}", r.stdout);
    assert!(r.stderr.contains("Path not found."), "{}", r.stderr);
}

#[test]
fn mkdir_invalid_name() {
    // base > 8, two dots, extension > 3
    let r = fresh("mkdir_invalid", "mkdir abcdefghi\nmkdir a.b.c\nmkdir a.abcd\nls\n");
    assert!(r.stdout.is_empty(), "{:?}", r.stdout);
    assert_eq!(r.stderr.matches("Invalid file name").count(), 3, "{}", r.stderr);
}

#[test]
fn mkdir_dir_full() {
    // 1024 B cluster / 32 B entry = 32 slots, `.` and `..` take two -> 30 free
    let commands: String = (0..31).map(|i| format!("mkdir d{i}\n")).collect();
    let r = fresh("mkdir_full", &commands);
    assert_eq!(r.stdout.len(), 30);
    assert!(r.stderr.contains("Directory is full."), "{}", r.stderr);
}

#[test]
fn mkdir_failure_does_not_leak_clusters() {
    let fs = fs_file("mkdir_no_leak");
    run(&fs, "format 1MB\nmkdir a1\n");
    let before = banner(&fs);

    run(&fs, "mkdir a1\nmkdir x/y\n");
    assert_eq!(banner(&fs), before);
}

// ===== rmdir ======

#[test]
fn rmdir_ok() {
    let r = fresh("rmdir_ok", "mkdir a1\nmkdir a2\nrmdir a1\nls\n");
    assert_eq!(r.stdout, ["OK", "OK", "OK", "DIR: a2"]);
}

#[test]
fn rmdir_not_found() {
    let r = fresh("rmdir_not_found", "rmdir x\n");
    assert!(r.stdout.is_empty(), "{:?}", r.stdout);
    assert!(r.stderr.contains("Path not found."), "{}", r.stderr);
}

#[test]
fn rmdir_not_empty() {
    let r = fresh("rmdir_not_empty", "mkdir a1\nmkdir a1/b1\nrmdir a1\nls\n");
    assert_eq!(r.stdout, ["OK", "OK", "DIR: a1"]);
    assert!(r.stderr.contains("Directory is not empty."), "{}", r.stderr);
}

#[test]
fn rmdir_root_and_cwd_not_allowed() {
    let r = fresh("rmdir_not_allowed", "mkdir a1\ncd a1\nrmdir .\nrmdir /\npwd\n");
    assert_eq!(r.stdout, ["OK", "OK", "/a1"]);
    assert!(r.stderr.contains("cannot remove the current directory"), "{}", r.stderr);
    assert!(r.stderr.contains("cannot remove the root directory"), "{}", r.stderr);
}

#[test]
fn rmdir_frees_cluster() {
    let fs = fs_file("rmdir_frees");
    run(&fs, "format 1MB\n");
    let before = banner(&fs);

    run(&fs, "mkdir a1\nrmdir a1\n");
    assert_eq!(banner(&fs), before);
}
