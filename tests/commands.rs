//! Integrační testy: spouštějí binárku `pseudofat`, posílají jí příkazy

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};

use rstest::{fixture, rstest};

// Struktura kvůli perzistenci, dá se nad ní volat opakovaně
const BIN: &str = env!("CARGO_BIN_EXE_pseudofat");

struct TempFs {
    path: PathBuf,
}

impl TempFs {
    /// Vytvoří unikátní cestu v dočasné složce (soubor zatím nevzniká).
    fn new() -> Self {
        // Testy běží paralelně ve více vláknech, takže pořadové číslo musí
        // být atomické – fetch_add přečte a zvýší hodnotu jako jeden
        // nedělitelný krok a dvě vlákna nikdy nedostanou stejné číslo.
        static NEXT_ID: AtomicU32 = AtomicU32::new(0);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);

        let file = format!("pseudofat-test-{}-{id}.dat", std::process::id());
        TempFs {
            path: std::env::temp_dir().join(file),
        }
    }

    /// Spustí pseudofat nad tímto souborem, pošle mu `input` a počká na konec.
    fn run(&self, input: &str) -> Output {
        let mut child = Command::new(BIN)
            .arg(&self.path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("cannot start pseudofat");

        // take() vezme stdin z child a na konci bloku ho zahodí (drop),
        // čímž se zavře a program dostane konec vstupu. Chybu zápisu
        // ignorujeme: program mohl skončit (exit) dřív, než přečetl vše.
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(input.as_bytes());
        }

        child.wait_with_output().expect("pseudofat did not finish")
    }
}

impl Drop for TempFs {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[fixture]
fn fs() -> TempFs {
    TempFs::new()
}

/// Řádky standardního výstupu.
fn lines(output: &Output) -> Vec<String> {
    String::from_utf8_lossy(&output.stdout).lines().map(str::to_string).collect()
}

// ---------------------------------------------------------------------------
// Spuštění a ukončení
// ---------------------------------------------------------------------------

/// Bez parametru program nic nespustí a vypíše nápovědu na stderr.
#[test]
fn missing_argument_fails_with_usage() {
    let output = Command::new(BIN).output().unwrap();

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Usage"));
}

/// Program skončí úspěšně a nic nevypíše.
#[rstest]
#[case::exit("exit\n")]
#[case::exit_ignores_rest("exit\nfoo\n")]
#[case::exit_crlf("exit\r\nfoo\r\n")]
#[case::end_of_input("")]
#[case::blank_lines_then_eof("\n   \r\n\t\n")]
fn ends_cleanly(fs: TempFs, #[case] input: &str) {
    let output = fs.run(input);

    assert!(output.status.success());
    assert!(lines(&output).is_empty());
}

// ---------------------------------------------------------------------------
// Syntaktické chyby
// ---------------------------------------------------------------------------

/// Jeden chybný řádek vypíše právě jednu hlášku a program běží dál
#[rstest]
#[case::unknown_command("foo bar", "UNKNOWN COMMAND (foo)")]
#[case::unknown_command_crlf("foo\r", "UNKNOWN COMMAND (foo)")]
#[case::unknown_command_spaces("  foo \t bar  ", "UNKNOWN COMMAND (foo)")]
#[case::missing_argument("cp s1", "WRONG ARGUMENTS (usage: cp <s1> <s2>)")]
#[case::extra_argument("ls a b", "WRONG ARGUMENTS (usage: ls [a1])")]
#[case::argument_not_expected("pwd x", "WRONG ARGUMENTS (usage: pwd)")]
#[case::size_not_number("format abc", "INVALID SIZE (abc, expected e.g. 600MB)")]
#[case::size_wrong_unit("format 12XB", "INVALID SIZE (12XB, expected e.g. 600MB)")]
fn syntax_error(fs: TempFs, #[case] line: &str, #[case] expected: &str) {
    let output = fs.run(&format!("{line}\nexit\n"));

    assert_eq!(lines(&output), [expected]);
}
