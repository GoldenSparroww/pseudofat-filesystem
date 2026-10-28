# PseudoFAT

A simplified FAT-based filesystem, written in Rust as a semester project for KIV/ZOS. The whole
filesystem is stored in a single file on the host disk and its content persists between runs.

## Usage

```sh
make build              # release binary into dist/
dist/pseudofat myfs.dat # or: make run
```

The program reads commands from standard input, one per line. An unformatted filesystem accepts
only `format`.

```text
format 10MB
mkdir a1
incp file.txt a1/f.txt
info a1/f.txt
```

## Commands

`cp`, `mv`, `rm`, `mkdir`, `rmdir`, `ls`, `cat`, `cd`, `pwd`, `info`, `incp`, `outcp`, `load`,
`format`, `exit`, plus `check` and `bug <s1> [size|cycle|invalid]`.

The script for `load` has to be inside our filesystem, copy it there with `incp` first.

## Makefile

| target        | what it does                                  |
| ------------- | --------------------------------------------- |
| `make build`  | `cargo build --release` and copy into `dist/` |
| `make run`    | runs `dist/pseudofat ./dist/myfs.dat`         |
| `make test`   | `cargo test` (integration tests in `tests/`)  |
| `make fmt`    | `cargo fmt`                                   |
| `make clippy` | `cargo clippy --all-targets`                  |
| `make clean`  | removes `target/` and `dist/`                 |

## Documentation

Detailed documentation (design, disk layout, implementation decisions) is in Czech:
[docs/cz/documentation.pdf](docs/cz/documentation.pdf)
(source: [docs/cz/documentation.tex](docs/cz/documentation.tex)).
