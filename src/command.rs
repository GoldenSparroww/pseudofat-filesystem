use std::fmt;

use crate::filesystem::{BugKind, FsError};

pub mod bug;
pub mod cat;
pub mod cd;
pub mod check;
pub mod cp;
pub mod format;
pub mod incp;
pub mod info;
pub mod load;
pub mod ls;
pub mod mkdir;
pub mod mv;
pub mod outcp;
pub mod pwd;
pub mod rm;
pub mod rmdir;

// ===== Command definitions ======

#[derive(Debug)]
pub enum Command {
    Cp { src: String, dst: String },
    Mv { src: String, dst: String },
    Rm { path: String },
    Mkdir { path: String },
    Rmdir { path: String },
    Ls { path: Option<String> },
    Cat { path: String },
    Cd { path: String },
    Pwd,
    Info { path: String },
    Incp { host_src: String, dst: String },
    Outcp { src: String, host_dst: String },
    Load { path: String },
    Format { size_raw: String },
    Exit,
    Check,
    Bug { path: String, kind: Option<BugKind> },
}

/// Syntax of all commands
const USAGE: &[(&str, &str)] = &[
    ("cp", "cp <s1> <s2>"),
    ("mv", "mv <s1> <s2>"),
    ("rm", "rm <s1>"),
    ("mkdir", "mkdir <a1>"),
    ("rmdir", "rmdir <a1>"),
    ("ls", "ls [a1]"),
    ("cat", "cat <s1>"),
    ("cd", "cd <a1>"),
    ("pwd", "pwd"),
    ("info", "info <s1|a1>"),
    ("incp", "incp <host-s1> <s2>"),
    ("outcp", "outcp <s1> <host-s2>"),
    ("load", "load <s1>"),
    ("format", "format <size>MB"),
    ("exit", "exit"),
    ("check", "check"),
    ("bug", "bug <s1> [size|cycle|invalid]"),
];

pub enum ParseError {
    UnknownCommand(String),
    WrongArguments(&'static str),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::UnknownCommand(name) => write!(f, "UNKNOWN COMMAND ({name})"),
            ParseError::WrongArguments(usage) => write!(f, "WRONG ARGUMENTS (usage: {usage})"),
        }
    }
}

impl Command {
    pub fn str_to_command(line: &str) -> Result<Option<Command>, ParseError> {
        // split_whitespace() automatically takes care of `\n`, `\r\n` => IDC about ending types
        let tokens: Vec<&str> = line.split_whitespace().collect();

        let command = match tokens.as_slice() {
            // Empty
            [] => return Ok(None),

            ["cp", src, dst] => Command::Cp { src: src.to_string(), dst: dst.to_string() },
            ["mv", src, dst] => Command::Mv { src: src.to_string(), dst: dst.to_string() },
            ["rm", path] => Command::Rm { path: path.to_string() },
            ["mkdir", path] => Command::Mkdir { path: path.to_string() },
            ["rmdir", path] => Command::Rmdir { path: path.to_string() },
            ["ls"] => Command::Ls { path: None },
            ["ls", path] => Command::Ls { path: Some(path.to_string()) },
            ["cat", path] => Command::Cat { path: path.to_string() },
            ["cd", path] => Command::Cd { path: path.to_string() },
            ["pwd"] => Command::Pwd,
            ["info", path] => Command::Info { path: path.to_string() },
            ["incp", host_src, dst] => Command::Incp { host_src: host_src.to_string(), dst: dst.to_string() },
            ["outcp", src, host_dst] => Command::Outcp { src: src.to_string(), host_dst: host_dst.to_string() },
            ["load", path] => Command::Load { path: path.to_string() },
            ["format", size_raw] => Command::Format { size_raw: size_raw.to_string() },
            ["exit"] => Command::Exit,
            ["check"] => Command::Check,
            ["bug", path] => Command::Bug { path: path.to_string(), kind: None },
            ["bug", path, kind] => Command::Bug {
                path: path.to_string(),
                kind: Some(BugKind::parse(kind).ok_or_else(|| unknown_or_wrong_arguments("bug"))?),
            },
            [name, ..] => return Err(unknown_or_wrong_arguments(name)),
        };

        Ok(Some(command))
    }
}

fn unknown_or_wrong_arguments(name: &str) -> ParseError {
    match USAGE.iter().find(|(command, _)| *command == name) {
        Some((_, usage)) => ParseError::WrongArguments(usage),
        None => ParseError::UnknownCommand(name.to_string()),
    }
}

// ===== Command output ======

pub enum Output {
    Ok,
    RawText(String),
}

// ===== CommandExecError ======

pub enum CommandExecError {
    // Specific
    FormatInvalidSize(String),
    FormatCannotCreate(FsError),
    RmdirNotEmpty,

    // General
    AlreadyHandled(String),
    NotAllowed(String),
    Fs(FsError), // Wrapper around FsError, which occured on command level
}

impl fmt::Display for CommandExecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandExecError::FormatInvalidSize(text) => {
                write!(f, "INVALID SIZE ({text}, expected e.g. 600MB)")
            }
            CommandExecError::FormatCannotCreate(e) => write!(f, "CANNOT CREATE FILE ({e})"),
            CommandExecError::AlreadyHandled(e) => write!(f, "{e}"),
            CommandExecError::Fs(e) => write!(f, "{e}"),
            CommandExecError::RmdirNotEmpty => write!(f, "NOT EMPTY"),
            CommandExecError::NotAllowed(msg) => write!(f, "Not allowed: {msg}"),
        }
    }
}

impl From<FsError> for CommandExecError {
    fn from(e: FsError) -> Self {
        CommandExecError::Fs(e)
    }
}
