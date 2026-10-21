# Obscure XMC Converter
A CLI tool for converting .xmc to .xml — and .xml to .xmc — from Obscure 1 and Obscure 2.

# How to Compile
## Windows
Install Rust, and in that folder, run:

```powershell
cargo build --release
```

The executable will be located in:

`target\release\obscure-xmc-converter.exe`

## Linux
**Install Rust**

Open the terminal and run:
```bash
sudo apt update
sudo apt install build-essential curl
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Follow the installer's instructions. Then, load the Rust environment:
```bash
source "$HOME/.cargo/env"
```

Check to see if it's installed:
```bash
rustc --version
cargo --version
```

**Compile the project**
```bash
cargo build --release
```

The executable will be located in:

`target/release/obscure-xmc-converter`

## Use via terminal
**Windows**
```bash
obscure-xmc-converter.exe extract filename.xmc
obscure-xmc-converter.exe build filename.xml
```

**Linux**
```bash
obscure-xmc-converter extract filename.xmc
obscure-xmc-converter build filename.xml
```

or

```bash
./obscure-xmc-converter extract filename.xmc
./obscure-xmc-converter build filename.xml
```

## Notes

- The binary format is treated as little-endian, with a 12-byte header, 16-byte node records, and 8-byte attributes.
- The format described does not represent XML text as node content; this conversion also preserves only elements and attributes.
- The XML-to-XMC conversion is a reconstruction of the format; it does not guarantee that the resulting bytes will be identical to those in the original file. Test on copies of the files and verify them in the game before replacing any files.
