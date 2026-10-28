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
    let path = tmp_path(&format!("{name}.dat"));
    let _ = std::fs::remove_file(&path);
    path
}

/// Path in cargo's tmp dir, where the binary runs (see `run`)
fn tmp_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR")).join(name)
}

/// Creates a file on the host for `incp`. Returns just its name, because the binary runs in the tmp dir
/// (the full path may contain spaces, which the command parser would split)
fn host_file(name: &str, content: &[u8]) -> String {
    std::fs::write(tmp_path(name), content).unwrap();
    name.to_owned()
}

/// Content spanning several clusters (cluster = 1024 B), one line without `\n`
fn big_content(len: usize) -> Vec<u8> {
    (0..len).map(|i| b'a' + (i % 26) as u8).collect()
}

/// Runs the binary over `fs` with `input` as stdin
fn run(fs: &Path, input: &str) -> Run {
    let mut child = Command::new(env!("CARGO_BIN_EXE_pseudofat"))
        .arg(fs)
        .current_dir(env!("CARGO_TARGET_TMPDIR"))
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
    assert!(r.stderr.contains("PATH NOT FOUND"), "{}", r.stderr);
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
    assert!(r.stderr.contains("PATH NOT FOUND"), "{}", r.stderr);
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
    assert_eq!(r.stderr.matches("EXIST").count(), 3, "{}", r.stderr);
}

#[test]
fn mkdir_path_not_found() {
    let r = fresh("mkdir_not_found", "mkdir x/y\n");
    assert!(r.stdout.is_empty(), "{:?}", r.stdout);
    assert!(r.stderr.contains("PATH NOT FOUND"), "{}", r.stderr);
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
    assert!(r.stderr.contains("PATH NOT FOUND"), "{}", r.stderr);
}

#[test]
fn rmdir_not_empty() {
    let r = fresh("rmdir_not_empty", "mkdir a1\nmkdir a1/b1\nrmdir a1\nls\n");
    assert_eq!(r.stdout, ["OK", "OK", "DIR: a1"]);
    assert!(r.stderr.contains("NOT EMPTY"), "{}", r.stderr);
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

// ===== incp, cat ======

#[test]
fn incp_and_cat() {
    let small = host_file("incp_cat_small.txt", b"ahoj svete");
    let big = big_content(3000);
    let big_name = host_file("incp_cat_big.txt", &big);

    let r = fresh("incp_cat", &format!("incp {small} s.txt\nincp {big_name} b.txt\ncat s.txt\ncat b.txt\n"));
    assert_eq!(r.stdout, ["OK", "OK", "ahoj svete", &String::from_utf8(big).unwrap()]);
}

#[test]
fn incp_errors() {
    let file = host_file("incp_errors.txt", b"x");
    let r = fresh("incp_errors", &format!("incp missing.txt a\nincp {file} x/a\nincp {file} a\nincp {file} a\n"));
    assert_eq!(r.stdout, ["OK"]);
    assert!(r.stderr.contains("FILE NOT FOUND"), "{}", r.stderr);
    assert!(r.stderr.contains("PATH NOT FOUND"), "{}", r.stderr);
    assert!(r.stderr.contains("EXIST"), "{}", r.stderr);
}

#[test]
fn cat_not_found() {
    // missing file, a directory instead of a file
    let r = fresh("cat_not_found", "mkdir a1\ncat missing\ncat a1\n");
    assert_eq!(r.stdout, ["OK"]);
    assert_eq!(r.stderr.matches("FILE NOT FOUND").count(), 2, "{}", r.stderr);
}

// ===== outcp ======

#[test]
fn outcp_multi_cluster_roundtrip() {
    let big = big_content(3000);
    let src = host_file("outcp_src.txt", &big);
    let _ = std::fs::remove_file(tmp_path("outcp_dst.txt"));

    let r = fresh("outcp_ok", &format!("incp {src} b.txt\noutcp b.txt outcp_dst.txt\n"));
    assert_eq!(r.stdout, ["OK", "OK"]);
    assert_eq!(std::fs::read(tmp_path("outcp_dst.txt")).unwrap(), big);
}

#[test]
fn outcp_errors() {
    let src = host_file("outcp_errors.txt", b"x");
    let r = fresh("outcp_errors", &format!("incp {src} a\noutcp missing out.txt\noutcp a missing_dir/out.txt\n"));
    assert_eq!(r.stdout, ["OK"]);
    assert!(r.stderr.contains("FILE NOT FOUND"), "{}", r.stderr);
    assert!(r.stderr.contains("PATH NOT FOUND"), "{}", r.stderr);
}

// ===== cp ======

#[test]
fn cp_ok() {
    let src = host_file("cp_ok.txt", &big_content(3000));
    let r = fresh("cp_ok", &format!("incp {src} a.txt\nmkdir a1\ncp a.txt a1/b.txt\nls a1\ncat a1/b.txt\n"));
    assert_eq!(&r.stdout[..4], ["OK", "OK", "OK", "FILE: b.txt"]);
    assert_eq!(r.stdout[4].as_bytes(), big_content(3000));
}

#[test]
fn cp_errors() {
    let src = host_file("cp_errors.txt", b"x");
    let r = fresh("cp_errors", &format!("incp {src} a\ncp missing b\ncp a x/b\ncp a a\n"));
    assert_eq!(r.stdout, ["OK"]);
    assert!(r.stderr.contains("FILE NOT FOUND"), "{}", r.stderr);
    assert!(r.stderr.contains("PATH NOT FOUND"), "{}", r.stderr);
    assert!(r.stderr.contains("EXIST"), "{}", r.stderr);
}

// ===== mv ======

#[test]
fn mv_rename_and_move() {
    let src = host_file("mv_ok.txt", b"data");
    let r = fresh(
        "mv_ok",
        &format!("incp {src} a.txt\nmkdir a1\nmv a.txt b.txt\nmv b.txt a1/c.txt\nls\nls a1\ncat a1/c.txt\n"),
    );
    assert_eq!(r.stdout, ["OK", "OK", "OK", "OK", "DIR: a1", "FILE: c.txt", "data"]);
}

#[test]
fn mv_dir_updates_parent() {
    // After the move, `..` of b1 has to point to the root
    let r = fresh("mv_dir", "mkdir a1\nmkdir a1/b1\nmv a1/b1 c1\ncd c1\npwd\ncd ..\npwd\n");
    assert_eq!(r.stdout, ["OK", "OK", "OK", "OK", "/c1", "OK", "/"]);
}

#[test]
fn mv_errors() {
    let src = host_file("mv_errors.txt", b"x");
    let r = fresh(
        "mv_errors",
        &format!("incp {src} a\nmkdir a1\nmkdir a1/b1\nmv missing b\nmv a x/b\nmv a a1\nmv a1 a1/b1/a1\n"),
    );
    assert_eq!(r.stdout, ["OK", "OK", "OK"]);
    assert!(r.stderr.contains("FILE NOT FOUND"), "{}", r.stderr);
    assert!(r.stderr.contains("PATH NOT FOUND"), "{}", r.stderr);
    assert!(r.stderr.contains("EXIST"), "{}", r.stderr);
    assert!(r.stderr.contains("cannot move a directory into itself"), "{}", r.stderr);
}

// ===== rm ======

#[test]
fn rm_ok() {
    let src = host_file("rm_ok.txt", b"x");
    let r = fresh("rm_ok", &format!("incp {src} a\nrm a\nls\ncat a\n"));
    assert_eq!(r.stdout, ["OK", "OK"]);
    assert!(r.stderr.contains("FILE NOT FOUND"), "{}", r.stderr);
}

#[test]
fn rm_not_found() {
    // missing file, a directory instead of a file
    let r = fresh("rm_not_found", "mkdir a1\nrm missing\nrm a1\nls\n");
    assert_eq!(r.stdout, ["OK", "DIR: a1"]);
    assert_eq!(r.stderr.matches("FILE NOT FOUND").count(), 2, "{}", r.stderr);
}

#[test]
fn rm_frees_clusters() {
    let src = host_file("rm_frees.txt", &big_content(3000));
    let fs = fs_file("rm_frees");
    run(&fs, "format 1MB\n");
    let before = banner(&fs);

    run(&fs, &format!("incp {src} a\ncp a b\n"));
    assert_ne!(banner(&fs), before);

    run(&fs, "rm a\nrm b\n");
    assert_eq!(banner(&fs), before);
}

// ===== info ======

#[test]
fn info_file_and_dir() {
    // Fresh FS: root is cluster 0, the file takes the next three, the directory the one after
    let src = host_file("info.txt", &big_content(3000));
    let r = fresh("info", &format!("incp {src} b.txt\nmkdir a1\ninfo b.txt\ninfo a1\ninfo /\ninfo missing\n"));
    assert_eq!(
        r.stdout,
        [
            "OK",
            "OK",
            "b.txt – file – size: 3000B, on disk: 3072B - 1,2,3",
            "a1 – dir – cluster - 4",
            "/ – dir – cluster - 0"
        ]
    );
    assert!(r.stderr.contains("FILE NOT FOUND"), "{}", r.stderr);
}

// ===== load ======

#[test]
fn load_runs_script() {
    // A failing command (cat) does not stop the script
    let script = host_file("load_script.txt", b"mkdir a1\r\ncat missing\r\ncd a1\r\npwd\r\n");
    let r = fresh("load_ok", &format!("incp {script} s.txt\nload s.txt\n"));
    assert_eq!(r.stdout, ["OK", "OK", "OK", "/a1", "OK"]);
    assert!(r.stderr.contains("FILE NOT FOUND"), "{}", r.stderr);
}

#[test]
fn load_not_found() {
    // The script has to be in our FS (copied by incp), not on the host
    host_file("load_host.txt", b"pwd\n");
    let r = fresh("load_not_found", "load load_host.txt\n");
    assert!(r.stdout.is_empty(), "{:?}", r.stdout);
    assert!(r.stderr.contains("FILE NOT FOUND"), "{}", r.stderr);
}

// ===== check, bug ======

#[test]
fn check_ok() {
    let src = host_file("check_ok.txt", &big_content(3000));
    let r = fresh("check_ok", &format!("mkdir a1\nincp {src} a1/b.txt\ncheck\n"));
    assert_eq!(r.stdout, ["OK", "OK", "OK"]);
}

#[test]
fn bug_kinds_detected_by_check() {
    let src = host_file("bug_kinds.txt", &big_content(3000));
    let r = fresh(
        "bug_kinds",
        &format!(
            "incp {src} s.txt\nincp {src} c.txt\nincp {src} i.txt\nbug s.txt size\nbug c.txt cycle\nbug i.txt invalid\ncheck\n"
        ),
    );
    assert_eq!(
        &r.stdout[3..6],
        ["Successfully damaged (size)", "Successfully damaged (cycle)", "Successfully damaged (invalid)"]
    );
    assert_eq!(r.stdout.len(), 9, "{:?}", r.stdout);
    assert!(r.stdout[6].starts_with("/s.txt: size"), "{:?}", r.stdout);
    assert!(r.stdout[7].starts_with("/c.txt:") && r.stdout[7].contains("cycle"), "{:?}", r.stdout);
    assert!(r.stdout[8].starts_with("/i.txt:") && r.stdout[8].contains("invalid cluster"), "{:?}", r.stdout);
}

#[test]
fn bug_random_is_detected() {
    let src = host_file("bug_random.txt", &big_content(3000));
    let r = fresh("bug_random", &format!("incp {src} f.txt\nbug f.txt\ncheck\n"));
    assert_eq!(r.stdout.len(), 3, "{:?}", r.stdout);
    assert!(r.stdout[1].starts_with("Successfully damaged ("), "{:?}", r.stdout);
    assert!(r.stdout[2].starts_with("/f.txt:"), "{:?}", r.stdout);
}

#[test]
fn bug_errors() {
    let empty = host_file("bug_empty.txt", b"");
    let r = fresh(
        "bug_errors",
        &format!("mkdir a1\nincp {empty} e.txt\nbug missing\nbug a1\nbug e.txt cycle\nbug e.txt free\n"),
    );
    assert_eq!(r.stdout, ["OK", "OK", "WRONG ARGUMENTS (usage: bug <s1> [size|cycle|invalid])"]);
    assert_eq!(r.stderr.matches("FILE NOT FOUND").count(), 2, "{}", r.stderr);
    // An empty file has no clusters, so only `size` can be applied
    assert!(r.stderr.contains("cannot apply cycle"), "{}", r.stderr);
}
