//! Interactive shell for managing pseudofat filesystem.

use std::io::{self, BufRead, IsTerminal, Write};
use std::path::PathBuf;

use crate::command::CommandExecError::{AlreadyHandled, NotImplemented};
use crate::command::{Command, format::do_format};
use crate::command::{CommandExecError, Output};
use crate::filesystem::{FileSystem, FsError};

enum Flow {
    Continue,
    Exit,
}

#[derive(Debug)]
pub struct Shell {
    fs_host_filepath: PathBuf,
    fs: Option<FileSystem>,
}

impl Shell {
    pub fn new(fs_host_filepath: PathBuf) -> Self {
        let fs = match FileSystem::try_open(&fs_host_filepath) {
            Ok(fs) => {
                println!(
                    "{}: filesystem loaded ({}, {})",
                    fs_host_filepath.display(),
                    fs.info_superblock(),
                    fs.info_fat()
                );
                Some(fs)
            }
            Err(e) => {
                eprintln!("{}: {}", fs_host_filepath.display(), e);
                None
            }
        };

        Shell { fs_host_filepath, fs }
    }

    /// Main loop
    pub fn run(&mut self) {
        // Show prompt if stidin is terminal (otherwise, it would make output messy)
        let interactive = io::stdin().is_terminal();
        let mut stdin = io::stdin().lock();
        let mut buf = Vec::new();

        loop {
            if interactive {
                print!("pseudofat> ");
                // print! does not clear the buffer on its own
                let _ = io::stdout().flush();
            }

            buf.clear();

            // CRLF, LF both have \n
            match stdin.read_until(b'\n', &mut buf) {
                // Contains EOF, end
                Ok(0) => {
                    if interactive {
                        println!();
                    }
                    break;
                }
                Ok(_) => {}
                // Almost never happens, just on OS errors
                Err(e) => {
                    eprintln!("ERROR: cannot read input: {e}");
                    break;
                }
            }

            // Until now, UTF-8 has not been checked. Valid UTF-8 is converted,
            // invalid characters are replaced with U+FFFD “�” (REPLACEMENT CHARACTER)
            let line = String::from_utf8_lossy(&buf);

            if let Flow::Exit = self.process_line(&line) {
                break;
            }
        }
    }

    /// Processes a single line: recognizes the command and executes it
    fn process_line(&mut self, line: &str) -> Flow {
        match Command::str_to_command(line) {
            Ok(None) => Flow::Continue,
            Ok(Some(command)) => self.execute(command),
            Err(e) => {
                println!("{e}");
                Flow::Continue
            }
        }
    }

    /// It will execute a syntactically correct command and display the result
    fn execute(&mut self, command: Command) -> Flow {
        let result = match command {
            Command::Exit => return Flow::Exit,
            // format is the only one that works even without a formatted file system
            Command::Format { size_raw } => do_format(&mut self.fs, &self.fs_host_filepath, &size_raw),
            // All other commands require FS — without it, they'll end up here as NOT FORMATTED
            command => self
                .require_fs()
                .map_err(|e| e.into())
                .and_then(|fs| run_fs_command(fs, command)),
        };

        match result {
            Ok(Output::Ok) => println!("OK"),
            Ok(Output::Lines(lines)) => {
                for line in lines {
                    println!("{line}");
                }
            }
            Ok(Output::RawText(text)) => {
                print!("{text}");
                if !text.is_empty() && !text.ends_with('\n') {
                    println!();
                }
            }
            Err(e) => eprintln!("{e}"),
        }

        Flow::Continue
    }

    /// Returns opened FS, or error, if not formatted
    fn require_fs(&mut self) -> Result<&mut FileSystem, FsError> {
        self.fs.as_mut().ok_or(FsError::NotFormatted)
    }
}

/// Vykoná příkaz nad otevřeným FS a vrátí výsledek k vypsání
fn run_fs_command(fs: &mut FileSystem, command: Command) -> Result<Output, CommandExecError> {
    match command {
        /*Command::Cp { src, dst } => fs.cp(&src, &dst).map(|()| Output::Ok),
        Command::Mv { src, dst } => fs.mv(&src, &dst).map(|()| Output::Ok),
        Command::Rm { path } => fs.rm(&path).map(|()| Output::Ok),
        Command::Mkdir { path } => fs.mkdir(&path).map(|()| Output::Ok),
        Command::Rmdir { path } => fs.rmdir(&path).map(|()| Output::Ok),
        Command::Ls { path } => fs.ls(path.as_deref()).map(Output::Lines),
        Command::Cat { path } => {
            let data = fs.read_file(&path)?;
            Ok(Output::RawText(String::from_utf8_lossy(&data).into_owned()))
        }
        Command::Cd { path } => fs.cd(&path).map(|()| Output::Ok),
        Command::Pwd => fs.pwd().map(|path| Output::Lines(vec![path])),
        Command::Info { path } => fs.info(&path).map(Output::Lines),
        Command::Incp { host_src, dst } => {
            let data = host::read_file(&parse_host_path(&host_src))?;
            fs.create_file(&dst, &data).map(|()| Output::Ok)
        }
        Command::Outcp { src, host_dst } => {
            let data = fs.read_file(&src)?;
            host::write_file(&parse_host_path(&host_dst), &data).map(|()| Output::Ok)
        }
        Command::Check => fs.check().map(Output::Lines),
        Command::Bug { path, kind } => fs.bug(&path, kind).map(|done| Output::Lines(vec![format!("OK ({done})")])),*/
        // Shell::execute should run these before
        Command::Load { .. } | Command::Format { .. } | Command::Exit => {
            Err(AlreadyHandled(String::from("Internal fatal error, attempt to execute command twice.")))
        }
        _ => Err(NotImplemented),
    }
}
