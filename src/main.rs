//! pseudoFAT – simplified filesystem based on FAT.
//!
//! Whole FS is saved on host as file. Program runs with one arg,
//! path to this file.
//!
//! ```sh
//! pseudofat myfs.dat
//! ```

mod bytes;
mod command;
mod config;
mod filesystem;
mod helpers;
mod host;
mod shell;

use std::env;
use std::ffi::OsString;
use std::process::ExitCode;

use shell::Shell;

fn main() -> ExitCode {
    // args_os: linux UTF-8, win UTF-16
    let args: Vec<OsString> = env::args_os().collect();

    // Only one param
    let [_, fs_path] = args.as_slice() else {
        eprintln!("Usage: pseudofat <fs-file>");
        return ExitCode::FAILURE;
    };

    let mut shell = Shell::new(host::parse_host_path(fs_path));
    shell.run();

    ExitCode::SUCCESS
}
