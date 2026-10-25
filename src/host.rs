//! Soubory na hostitelském disku (mimo pseudoFAT): parametr programu, incp, outcp, load
//!
//! Cesty se tu zapisují podle hostitelského systému (`C:\data\a.txt` na Windows,
//! `/home/user/a.txt` na Linuxu), relativní cesta se vztahuje k pracovnímu adresáři programu

use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::error::FsError;

/// OS specific string na instanci chytré cesty
pub fn host_path(text: impl AsRef<OsStr>) -> PathBuf {
    PathBuf::from(text.as_ref())
}

/// Přečte celý soubor na hostiteli a vrátí jako pole bajtů
pub fn read_file(path: &Path) -> Result<Vec<u8>, FsError> {
    // fs::read by na adresáři selhal chybou, která se liší podle systému
    if path.is_dir() {
        return Err(FsError::FileNotFound);
    }

    fs::read(path).map_err(|e| match e.kind() {
        io::ErrorKind::NotFound | io::ErrorKind::NotADirectory => FsError::FileNotFound,
        _ => FsError::Io(e),
    })
}

/// Vytvoří na hostiteli nový soubor s obsahem `data`
pub fn write_file(path: &Path, data: &[u8]) -> Result<(), FsError> {
    // create_new vytvoří soubor, jen když ještě neexistuje
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| match e.kind() {
            io::ErrorKind::AlreadyExists => FsError::Exist,
            io::ErrorKind::NotFound | io::ErrorKind::NotADirectory => FsError::PathNotFound,
            _ => FsError::Io(e),
        })?;

    if let Err(e) = file.write_all(data) {
        // Neúplný soubor po sobě nenecháme (např. plný disk hostitele). Na Windows nejde
        // smazat otevřený soubor, proto se nejdřív zavře
        drop(file);
        let _ = fs::remove_file(path);
        return Err(e.into());
    }

    Ok(())
}
