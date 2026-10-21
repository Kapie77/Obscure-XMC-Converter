# Obscure XMC Converter
A tool for converting .xmc to .xml — and .xml to .xmc — from Obscure 1 and Obscure 2.

# How to Compile
## Windows
Install Rust, and in that folder, run:

```powershell
cargo build --release
```

The executable will be located in:
`target\release\xmc-rust-converter.exe`

## Linux
1. Install Rust
```bash
Open the terminal and run:
sudo apt update
sudo apt install build-essential curl
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Follow the installer's instructions. Then, load the Rust environment:
```bash
source "$HOME/.cargo/env"
``

Check to see if it's installed:
```bash
rustc --version
cargo --version
```

2. Compile the project
```bash
cargo build --release
```

The executable will be located in:
`target/release/xmc-rust-converter`

## Use via terminal
```powershell
cargo run --release -- "C:\Folder\file.xmc"
cargo run --release -- "C:\Folder\file.xml"
cargo run --release -- --dict "C:\Folder\hashes.json" "file.xmc"
cargo run --release -- --mode xmc2xml "file.xmc"
cargo run --release -- --mode xml2xmc "file.xml"
cargo run --release -- --debug "file.xmc"
```

For multiple files of the same type, enclose all paths in quotes. Use the `-o` option to specify an output folder:

```powershell
cargo run --release -- "a.xmc" "b.xmc" -o "C:\Converted XMC"
```

## Notes

- The binary format is treated as little-endian, with a 12-byte header, 16-byte node records, and 8-byte attributes.
- The format described does not represent XML text as node content; this conversion also preserves only elements and attributes.
- The XML-to-XMC conversion is a reconstruction of the format; it does not guarantee that the resulting bytes will be identical to those in the original file. Test on copies of the files and verify them in the game before replacing any files.
