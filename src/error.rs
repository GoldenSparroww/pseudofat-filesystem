//! Chyby souborového systému
//!
//! Syntaktické chyby (neznámý příkaz, špatný počet parametrů) řeší shell sám, sem patří
//! jen chyby, které vzniknou při vykonání příkazu

use std::fmt;
use std::io;

#[derive(Debug)]
pub enum FsError {
    /// Zdrojový soubor neexistuje (nebo to není soubor)
    FileNotFound,
    /// Cesta nebo adresář neexistuje
    PathNotFound,
    /// Vytvářená položka už existuje
    Exist,
    /// Mazaný adresář není prázdný
    NotEmpty,
    /// Soubor se souborovým systémem nejde vytvořit (format), nese důvod
    CannotCreateFile(String),

    // --- Další chyby ---
    /// Soubor zatím neobsahuje náš FS, povolený je jen format
    NotFormatted,
    /// Na disku není dost volných clusterů
    DiskFull,
    /// Data na disku si odporují, nese popis problému
    Corrupted(String),
    /// Chyba operačního systému při čtení nebo zápisu souboru se FS
    Io(io::Error),
}

impl fmt::Display for FsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FsError::FileNotFound => write!(f, "FILE NOT FOUND"),
            FsError::PathNotFound => write!(f, "PATH NOT FOUND"),
            FsError::Exist => write!(f, "EXIST"),
            FsError::NotEmpty => write!(f, "NOT EMPTY"),
            FsError::CannotCreateFile(reason) => write!(f, "CANNOT CREATE FILE ({reason})"),
            FsError::NotFormatted => write!(f, "NOT FORMATTED (use: format <size>MB)"),
            FsError::DiskFull => write!(f, "DISK FULL"),
            FsError::Corrupted(detail) => write!(f, "CORRUPTED FILESYSTEM ({detail})"),
            FsError::Io(e) => write!(f, "IO ERROR ({e})"),
        }
    }
}

/// Díky této implementaci převede operátor `?` chybu `io::Error` na `FsError` sám
impl From<io::Error> for FsError {
    fn from(e: io::Error) -> Self {
        FsError::Io(e)
    }
}
