//! Interaktivní shell.

use std::fmt;
use std::io::{self, BufRead, IsTerminal, Write};
use std::num::IntErrorKind;
use std::path::PathBuf;

use crate::config::FORMAT_UNIT;
use crate::error::FsError;
use crate::filesystem::FileSystem;

/// Syntakticky správný příkaz včetně parametrů
#[derive(Debug)]
pub enum Command {
    Cp {
        src: String,
        dst: String,
    },
    Mv {
        src: String,
        dst: String,
    },
    Rm {
        path: String,
    },
    Mkdir {
        path: String,
    },
    Rmdir {
        path: String,
    },
    Ls {
        path: Option<String>,
    },
    Cat {
        path: String,
    },
    Cd {
        path: String,
    },
    Pwd,
    Info {
        path: String,
    },
    Incp {
        host_src: String,
        dst: String,
    },
    Outcp {
        src: String,
        host_dst: String,
    },
    Load {
        host_path: String,
    },
    Format {
        size_bytes: u64,
    },
    Exit,
    Check,
    Bug {
        path: String,
    },
}

/// Syntaxe všech příkazů
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
    ("load", "load <host-s1>"),
    ("format", "format <size>MB"),
    ("exit", "exit"),
    ("check", "check"),
    ("bug", "bug <s1>"),
];

/// Syntaktická chyba
#[derive(Debug)]
pub enum ParseError {
    /// Neznámý název příkazu
    UnknownCommand(String),
    /// Známý příkaz se špatným počtem parametrů, nese správnou syntaxi
    WrongArguments(&'static str),
    /// Parametr příkazu `format` není velikost ve tvaru `600MB`
    InvalidSize(String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::UnknownCommand(name) => write!(f, "UNKNOWN COMMAND ({name})"),
            ParseError::WrongArguments(usage) => write!(f, "WRONG ARGUMENTS (usage: {usage})"),
            ParseError::InvalidSize(text) => {
                write!(f, "INVALID SIZE ({text}, expected e.g. 600MB)")
            }
        }
    }
}

/// Převede jeden řádek na příkaz
pub fn parse(line: &str) -> Result<Option<Command>, ParseError> {
    // split_whitespace() automaticky řeší i `\n`, `\r\n` => konce řádků neřešim
    let tokens: Vec<&str> = line.split_whitespace().collect();

    let command = match tokens.as_slice() {
        [] => return Ok(None),

        ["cp", src, dst] => Command::Cp {
            src: src.to_string(),
            dst: dst.to_string(),
        },
        ["mv", src, dst] => Command::Mv {
            src: src.to_string(),
            dst: dst.to_string(),
        },
        ["rm", path] => Command::Rm { path: path.to_string() },
        ["mkdir", path] => Command::Mkdir { path: path.to_string() },
        ["rmdir", path] => Command::Rmdir { path: path.to_string() },
        ["ls"] => Command::Ls { path: None },
        ["ls", path] => Command::Ls {
            path: Some(path.to_string()),
        },
        ["cat", path] => Command::Cat { path: path.to_string() },
        ["cd", path] => Command::Cd { path: path.to_string() },
        ["pwd"] => Command::Pwd,
        ["info", path] => Command::Info { path: path.to_string() },
        ["incp", host_src, dst] => Command::Incp {
            host_src: host_src.to_string(),
            dst: dst.to_string(),
        },
        ["outcp", src, host_dst] => Command::Outcp {
            src: src.to_string(),
            host_dst: host_dst.to_string(),
        },
        ["load", host_path] => Command::Load {
            host_path: host_path.to_string(),
        },
        ["format", size] => Command::Format {
            size_bytes: parse_size(size)?,
        },
        ["exit"] => Command::Exit,
        ["check"] => Command::Check,
        ["bug", path] => Command::Bug { path: path.to_string() },

        // Nic nesedí: buď neznámý příkaz, nebo známý se špatným počtem parametrů
        [name, ..] => return Err(unknown_or_wrong_arguments(name)),
    };

    Ok(Some(command))
}

/// Rozliší neznámý příkaz od známého příkazu se špatnými parametry
fn unknown_or_wrong_arguments(name: &str) -> ParseError {
    match USAGE.iter().find(|(command, _)| *command == name) {
        Some((_, usage)) => ParseError::WrongArguments(usage),
        None => ParseError::UnknownCommand(name.to_string()),
    }
}

/// Převede velikost ve tvaru `600MB` (nebo jen `600`) na bajty
fn parse_size(text: &str) -> Result<u64, ParseError> {
    let upper = text.to_ascii_uppercase();
    let number = upper.strip_suffix("MB").unwrap_or(&upper);

    let units = match number.parse::<u64>() {
        Ok(n) => n,
        // Syntakticky správné číslo, jen se nevejde do u64 (víc než 20 číslic)
        Err(e) if *e.kind() == IntErrorKind::PosOverflow => u64::MAX,
        Err(_) => return Err(ParseError::InvalidSize(text.to_string())),
    };

    // saturating_mul: při přetečení vrátí u64::MAX místo panic/přetečení
    Ok(units.saturating_mul(FORMAT_UNIT))
}

/// Co má shell udělat po vykonání příkazu
enum Flow {
    Continue,
    Exit,
}

/// Stav běžícího shellu
pub struct Shell {
    /// Cesta k souboru se souborovým systémem (parametr programu)
    fs_path: PathBuf,
    /// Otevřený FS, `None` dokud soubor neobsahuje platný FS (pak funguje jen format)
    fs: Option<FileSystem>,
}

impl Shell {
    /// Otevře FS ze souboru `fs_path`, pokud v něm nějaký je
    pub fn new(fs_path: PathBuf) -> Self {
        // Hlášení při startu jdou na stderr: nejsou odpovědí na žádný příkaz
        let fs = match FileSystem::open(&fs_path) {
            Ok(fs) => {
                eprintln!(
                    "{}: filesystem loaded ({}, {} free)",
                    fs_path.display(),
                    fs.superblock(),
                    fs.free_clusters()
                );
                Some(fs)
            }
            Err(e) => {
                eprintln!("{}: {e}", fs_path.display());
                None
            }
        };

        Shell { fs_path, fs }
    }

    /// Hlavní smyčka
    pub fn run(&mut self) {
        // Výzvu zobrazujeme jen při ručním psaní (u testů špiní výstup)
        let interactive = io::stdin().is_terminal();
        let mut stdin = io::stdin().lock();
        let mut buf = Vec::new();

        loop {
            if interactive {
                print!("pseudofat> ");
                // print! sám nevyprázdní buffer – bez flush by se výzva nezobrazila
                let _ = io::stdout().flush();
            }

            buf.clear();
            // Neřeším \r\n, protože mě zajímá konec, a ten je všude stejný (pwd\n, pwd\r\n)
            // Proč ne jednodušší read_line(&mut String)? Vyžaduje platné UTF-8
            // Neřeší UTF-16 ani jiné, čte jen bajty až do \n
            match stdin.read_until(b'\n', &mut buf) {
                // Zahrnuje EOF
                Ok(0) => {
                    // Konec vstupu: odřádkujeme, aby terminál nezůstal za výzvou
                    if interactive {
                        println!();
                    }
                    break;
                }
                Ok(_) => {}
                // Typicky skoro nikdy nenastane, jen při chybách OS apod
                Err(e) => {
                    eprintln!("ERROR: cannot read input: {e}");
                    break;
                }
            }

            // Do teď se UTF-8 nehlídalo. Platné UTF-8 převede,
            // nevalidní nahradí za U+FFFD "�" (REPLACEMENT CHARACTER)
            let line = String::from_utf8_lossy(&buf);

            if let Flow::Exit = self.process_line(&line) {
                break;
            }
        }
    }

    /// Zpracuje jeden řádek: rozpozná příkaz a vykoná ho
    fn process_line(&mut self, line: &str) -> Flow {
        match parse(line) {
            Ok(None) => Flow::Continue,
            Ok(Some(command)) => self.execute(command),
            Err(e) => {
                println!("{e}");
                Flow::Continue
            }
        }
    }

    /// Vykoná syntakticky správný příkaz a vypíše výsledek
    fn execute(&mut self, command: Command) -> Flow {
        let result = match command {
            Command::Exit => return Flow::Exit,
            Command::Format { size_bytes } => self.format(size_bytes),
            _ => self.not_implemented(),
        };

        // Sémantické chyby jsou odpovědí na příkaz stejně jako OK – patří na stdout
        if let Err(e) = result {
            println!("{e}");
        }

        Flow::Continue
    }

    /// Vrátí otevřený FS, nebo chybu, pokud ještě není naformátovaný
    fn require_fs(&mut self) -> Result<&mut FileSystem, FsError> {
        self.fs.as_mut().ok_or(FsError::NotFormatted)
    }

    /// format: přepíše soubor novým prázdným FS
    fn format(&mut self, size_bytes: u64) -> Result<(), FsError> {
        // Starý FS zavřeme, aby soubor nebyl při přepisování otevřený dvakrát
        self.fs = None;

        match FileSystem::format(&self.fs_path, size_bytes) {
            Ok(fs) => {
                self.fs = Some(fs);
                println!("OK");
                Ok(())
            }
            Err(e) => {
                // Format mohl selhat ještě před zásahem do souboru (nesmyslná velikost).
                // Původní FS pak zůstal netknutý, takže ho zkusíme otevřít znovu
                self.fs = FileSystem::open(&self.fs_path).ok();
                Err(e)
            }
        }
    }

    /// Zástupce za zatím neimplementované příkazy
    fn not_implemented(&mut self) -> Result<(), FsError> {
        self.require_fs()?;
        println!("NOT IMPLEMENTED");
        Ok(())
    }
}
