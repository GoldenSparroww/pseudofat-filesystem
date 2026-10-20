//! pseudoFAT – zjednodušený souborový systém založený na FAT.
//!
//! Celý souborový systém je uložen v jednom souboru na hostitelském disku.
//! Program se spouští s jediným parametrem – cestou k tomuto souboru:
//!
//! ```text
//! pseudofat myfs.dat
//! ```

mod config;
mod error;
mod filesystem;
mod shell;

use std::env;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use shell::Shell;

fn main() -> ExitCode {
    // args_os místo args: cesta na hostiteli nemusí být platné UTF-8
    // a args() by na takové cestě spadl.
    let args: Vec<OsString> = env::args_os().collect();
    
    // Očekáváme přesně jeden parametr – soubor se souborovým systémem.
    let [_, fs_path] = args.as_slice() else {
        eprintln!("Usage: pseudofat <fs-file>");
        return ExitCode::FAILURE;
    };

    let mut shell = Shell::new(PathBuf::from(fs_path));
    shell.run();

    ExitCode::SUCCESS
}
