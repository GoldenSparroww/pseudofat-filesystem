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

    /// Velikost souboru se FS v bajtech
    fn size(&self) -> u64 {
        std::fs::metadata(&self.path).expect("fs file does not exist").len()
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

/// Výstup musí mít právě jeden řádek, vrátí ho
fn single_line(output: &Output) -> String {
    let mut out = lines(output);
    assert_eq!(out.len(), 1, "expected exactly one line, got: {out:?}");
    out.remove(0)
}

/// Řádek musí obsahovat všechny části – hlášky se tak dají upravovat, aniž by se rozbily testy
fn assert_contains(line: &str, parts: &[&str]) {
    for part in parts {
        assert!(line.contains(part), "{line:?} does not contain {part:?}");
    }
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
#[case::unknown_command("foo bar", &["UNKNOWN COMMAND", "(foo)"])]
#[case::unknown_command_crlf("foo\r", &["UNKNOWN COMMAND", "(foo)"])]
#[case::unknown_command_spaces("  foo \t bar  ", &["UNKNOWN COMMAND", "(foo)"])]
#[case::missing_argument("cp s1", &["WRONG ARGUMENTS", "cp <s1> <s2>"])]
#[case::extra_argument("ls a b", &["WRONG ARGUMENTS", "ls [a1]"])]
#[case::argument_not_expected("pwd x", &["WRONG ARGUMENTS", "pwd"])]
#[case::size_not_number("format abc", &["INVALID SIZE", "abc"])]
#[case::size_wrong_unit("format 12XB", &["INVALID SIZE", "12XB"])]
#[case::size_negative("format -5MB", &["INVALID SIZE", "-5MB"])]
#[case::size_unit_only("format MB", &["INVALID SIZE"])]
fn syntax_error(fs: TempFs, #[case] line: &str, #[case] expected: &[&str]) {
    let output = fs.run(&format!("{line}\nexit\n"));

    assert_contains(&single_line(&output), expected);
}

// ---------------------------------------------------------------------------
// Format a rozpoznání FS
// ---------------------------------------------------------------------------

/// Klíčové slovo hlášky pro příkazy zadané dřív, než soubor obsahuje FS
const NOT_FORMATTED: &str = "NOT FORMATTED";

/// Jednotka příkazu format, musí odpovídat `FORMAT_UNIT` v config.rs
const MB: u64 = 1024 * 1024;

/// Bez FS se odmítne každý příkaz kromě format a exit a soubor nevznikne
#[rstest]
fn rejected_before_format(fs: TempFs, #[values("ls", "pwd", "cat s1", "mkdir a1", "check")] command: &str) {
    let output = fs.run(&format!("{command}\nexit\n"));

    assert_contains(&single_line(&output), &[NOT_FORMATTED]);
    assert!(!fs.path.exists(), "only format may create the file");
}

/// format vytvoří soubor přesně zadané velikosti
#[rstest]
#[case::one_mb(1)]
#[case::ten_mb(10)]
#[case::six_hundred_mb(600)]
fn format_creates_file(fs: TempFs, #[case] megabytes: u64) {
    let output = fs.run(&format!("format {megabytes}MB\nexit\n"));

    assert_eq!(lines(&output), ["OK"]);
    assert_eq!(fs.size(), megabytes * MB);
}

/// Nesmyslná velikost se odmítne dřív, než se sáhne na soubor
#[rstest]
#[case::zero("0MB", "too small")]
// MAX_DISK_SIZE = 4 GiB = 4096 MB
#[case::above_limit("4097MB", "too large")]
#[case::multiplication_overflow("99999999999999MB", "too large")]
#[case::number_overflow("99999999999999999999999MB", "too large")]
fn format_rejects_size(fs: TempFs, #[case] size: &str, #[case] reason: &str) {
    let output = fs.run(&format!("format {size}\nexit\n"));

    assert_contains(&single_line(&output), &["CANNOT CREATE FILE", reason]);
    assert!(!fs.path.exists(), "invalid size must not create the file");
}

/// Soubor ve složce, která neexistuje, nejde vytvořit
#[test]
fn format_into_missing_directory() {
    let fs = TempFs {
        path: std::env::temp_dir().join("pseudofat-missing-dir").join("fs.dat"),
    };
    let output = fs.run("format 1MB\nexit\n");

    assert_contains(&single_line(&output), &["CANNOT CREATE FILE"]);
}

/// FS zůstane po ukončení programu v souboru a další spuštění ho rozpozná
#[rstest]
fn formatted_fs_survives_restart(fs: TempFs) {
    fs.run("format 10MB\nexit\n");
    let output = fs.run("pwd\nexit\n");

    // TODO krok 5: až bude pwd, porovnat přesně s ["/"]
    assert!(!single_line(&output).contains(NOT_FORMATTED));
    assert_eq!(fs.size(), 10 * MB);
}

/// Opakovaný format přepíše existující FS novým (i menším)
#[rstest]
fn reformat_replaces_fs(fs: TempFs) {
    let output = fs.run("format 10MB\nformat 2MB\nexit\n");

    assert_eq!(lines(&output), ["OK", "OK"]);
    assert_eq!(fs.size(), 2 * MB);
}

/// Neúspěšný format nechá původní FS funkční
#[rstest]
fn failed_format_keeps_old_fs(fs: TempFs) {
    let output = fs.run("format 2MB\nformat 0MB\npwd\nexit\n");

    let out = lines(&output);
    assert_eq!(out.len(), 3, "unexpected output: {out:?}");
    assert_eq!(out[0], "OK");
    assert_contains(&out[1], &["CANNOT CREATE FILE", "too small"]);
    assert!(!out[2].contains(NOT_FORMATTED), "old fs must stay open: {out:?}");
    assert_eq!(fs.size(), 2 * MB);
}

/// Soubor bez našeho FS se nerozpozná a zůstane nedotčený
#[rstest]
#[case::empty(b"")]
#[case::short_text(b"hello world\n")]
#[case::garbage(&[0xAB; 4096])]
fn foreign_file_is_not_formatted(fs: TempFs, #[case] content: &[u8]) {
    std::fs::write(&fs.path, content).unwrap();
    let output = fs.run("pwd\nexit\n");

    assert_contains(&single_line(&output), &[NOT_FORMATTED]);
    assert_eq!(
        std::fs::read(&fs.path).unwrap(),
        content,
        "foreign file must stay untouched"
    );
}

/// Superblok, který neodpovídá souboru, se při startu nahlásí jako poškozený
#[rstest]
fn truncated_fs_is_reported_as_corrupted(fs: TempFs) {
    fs.run("format 2MB\nexit\n");
    // Po zkrácení tvrdí superblok jinou velikost disku, než má soubor
    std::fs::OpenOptions::new()
        .write(true)
        .open(&fs.path)
        .unwrap()
        .set_len(MB)
        .unwrap();

    let output = fs.run("pwd\nexit\n");

    assert_contains(&single_line(&output), &[NOT_FORMATTED]);
    assert_contains(&String::from_utf8_lossy(&output.stderr), &["CORRUPTED"]);
}

// ---------------------------------------------------------------------------
// FAT a kořenový adresář (čtené přímo ze souboru)
// ---------------------------------------------------------------------------

/// Pozice polí superbloku (viz superblock.rs)
const SB_CLUSTER_SIZE: usize = 12;
const SB_CLUSTER_COUNT: usize = 16;
const SB_ROOT_CLUSTER: usize = 20;
const SB_FAT_START: usize = 32;
const SB_DATA_START: usize = 40;

/// Hodnota položky EOF na disku (u32::MAX - 1, little-endian)
const RAW_EOF: [u8; 4] = [0xFE, 0xFF, 0xFF, 0xFF];

/// Velikost položky adresáře a pozice jejích polí (viz dir.rs)
const DIR_ENTRY_SIZE: usize = 32;
const DE_KIND: usize = 12;
const DE_SIZE: usize = 16;
const DE_START: usize = 20;
const KIND_DIR: u8 = 2;

/// Číslo u32 (little-endian) na pozici `at`
fn read_u32(bytes: &[u8], at: usize) -> usize {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize
}

/// Číslo u64 (little-endian) na pozici `at`
fn read_u64(bytes: &[u8], at: usize) -> usize {
    u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap()) as usize
}

/// Po formátu je obsazený jen cluster kořenového adresáře (EOF), ostatní jsou volné
/// (FREE = u32::MAX = FF FF FF FF)
#[rstest]
#[case::one_mb(1)]
#[case::ten_mb(10)]
fn format_fat_contains_only_root(fs: TempFs, #[case] megabytes: u64) {
    fs.run(&format!("format {megabytes}MB\nexit\n"));

    let bytes = std::fs::read(&fs.path).unwrap();
    let count = read_u32(&bytes, SB_CLUSTER_COUNT);
    let root = read_u32(&bytes, SB_ROOT_CLUSTER);
    let fat = &bytes[read_u64(&bytes, SB_FAT_START)..][..count * 4];

    for (cluster, entry) in fat.chunks(4).enumerate() {
        if cluster == root {
            assert_eq!(entry, RAW_EOF, "root cluster {cluster} must be EOF");
        } else {
            assert_eq!(entry, [0xFF; 4], "cluster {cluster} must be FREE");
        }
    }
}

/// Při startu se FAT načte ze souboru: ručně obsazené položky ubydou z volných
#[rstest]
#[case::untouched(0)]
#[case::one_used(1)]
#[case::five_used(5)]
fn fat_is_read_from_disk(fs: TempFs, #[case] used: usize) {
    fs.run("format 1MB\nexit\n");

    let mut bytes = std::fs::read(&fs.path).unwrap();
    let count = read_u32(&bytes, SB_CLUSTER_COUNT);
    let start = read_u64(&bytes, SB_FAT_START);
    // Cluster 0 je kořen, obsazujeme až clustery za ním
    for cluster in 1..=used {
        let at = start + cluster * 4;
        bytes[at..at + 4].copy_from_slice(&RAW_EOF);
    }
    std::fs::write(&fs.path, &bytes).unwrap();

    let output = fs.run("exit\n");

    let free = count - 1 - used;
    assert_contains(&String::from_utf8_lossy(&output.stderr), &[&format!("{free} free")]);
}

/// Kořenový adresář obsahuje jen "." a "..", obě ukazují na kořen; zbytek clusteru je prázdný
#[rstest]
fn format_creates_root_directory(fs: TempFs) {
    fs.run("format 1MB\nexit\n");

    let bytes = std::fs::read(&fs.path).unwrap();
    let cluster_size = read_u32(&bytes, SB_CLUSTER_SIZE);
    let root = read_u32(&bytes, SB_ROOT_CLUSTER);
    let offset = read_u64(&bytes, SB_DATA_START) + root * cluster_size;
    let cluster = &bytes[offset..offset + cluster_size];

    for (slot, name) in [
        (0, &b".\0\0\0\0\0\0\0\0\0\0\0"[..]),
        (1, &b"..\0\0\0\0\0\0\0\0\0\0"[..]),
    ] {
        let entry = &cluster[slot * DIR_ENTRY_SIZE..][..DIR_ENTRY_SIZE];
        assert_eq!(&entry[..12], name, "slot {slot}: name");
        assert_eq!(entry[DE_KIND], KIND_DIR, "slot {slot}: kind");
        assert_eq!(read_u32(entry, DE_SIZE), 0, "slot {slot}: size");
        assert_eq!(read_u32(entry, DE_START), root, "slot {slot}: start");
    }
    assert!(
        cluster[2 * DIR_ENTRY_SIZE..].iter().all(|&b| b == 0),
        "other slots must be free"
    );
}
