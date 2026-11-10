# Obscure XMC Converter
A CLI tool for converting .xmc to .xml — and .xml to .xmc — from Obscure 1/2 and
Final Exam (all of which share the same XML vocabulary, in two different binary
containers).

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

## Typical use

```powershell
# one file, names resolved from the shipped dictionary
obscure-xmc-converter.exe extract file.xmc --dict hashes.json

# a whole tree at once (PowerShell splats the array)
$files = (Get-ChildItem .\xmc -Recurse -Filter *.xmc).FullName
obscure-xmc-converter.exe extract @files --output .\xml --dict hashes.json

# rebuild them all; the format is read from the XML marker
$xml = (Get-ChildItem .\xml -Recurse -Filter *.xml).FullName
obscure-xmc-converter.exe build @xml --output .\xmc --dict hashes.json
```

Outputs are named after the input file stem, so when converting a whole tree
into a single output directory, keep the source names unique.

The format is picked automatically, so the same commands work for Obscure 1/2
and Final Exam files.

## Notes

- The Obscure 1/2 binary format is treated as little-endian, with a 12-byte header, 16-byte node records, and 8-byte attributes.
- The format described does not represent XML text as node content; this conversion also preserves only elements and attributes.
- The XML-to-XMC conversion is a reconstruction of the format; it does not guarantee that the resulting bytes will be identical to those in the original file. Test on copies of the files and verify them in the game before replacing any files.

## Container formats

There are two layouts, both detected automatically:

| | Obscure 1 / 2 | Final Exam |
|---|---|---|
| Header | 12 bytes (`u32` node/attr/string counts) | 17 bytes (`u32` magic `0x66`, `u8` version `3`, three 24-bit counts) |
| Node | 16 bytes | 20 bytes (adds `u16` padding + `u32` `0xFFFFFFFF`) |
| String pool | plain CP1252 | encrypted with a keystream derived from the three counts |

`extract` sniffs the file: a Final Exam file has magic `0x66` and its counts add
up exactly to the file size. `build` reads the
`<!-- xmc-format: final-exam -->` comment that `extract` writes, and falls back
to the Obscure 1/2 layout for XML that has no such marker.

`--format auto` is the default; `--format obscure2` and `--format final-exam`
force one layout (useful for XML files written by hand).

The Final Exam pool cipher is reversible without a key: the keystream depends
only on the node, attribute and pool counts stored in the header.

## CRC dictionary

`hashes.json` maps tag/attribute name CRC32s to names. Unknown names are written
as `tag_XXXXXXXX` / `attr_XXXXXXXX` and round-trip unchanged, so a missing name
never loses data. 59 CRCs used by the shipped files are still unnamed, including
the root tag of the font files.

## Text encoding

The string pool is not UTF-8. The game stores text as CP1252 (Windows-1252),
one byte per character, so decoding it as UTF-8 turned every accented character
into `�` (U+FFFD).

- `extract` decodes each value as UTF-8 when the bytes are valid UTF-8, and as
  CP1252 otherwise, so `é`, `à`, `°`, `œ`, `€` and friends come out correctly.
- `build` writes CP1252 when every character fits, and UTF-8 otherwise.
- `--encoding cp1252` forces CP1252 and `--encoding utf8` forces UTF-8.
  Almost every shipped Obscure 2 file is CP1252, including the dialogue and the
  `font_nonserif12`/`font_smallfonts7` glyph tables. `font_arial14b.xmc` is the
  one exception: it stores its glyphs as UTF-8, so extracting and rebuilding it
  without edits converts them to CP1252 (same characters, one byte each instead
  of two).

Font glyphs (`<char>`) also carry a `code` attribute holding the Unicode code
point. Some shipped fonts store U+2122 (`™`) as the two bytes `84 A2`, which is
the UTF-8 sequence `E2 84 A2` with its lead byte missing. `extract` rebuilds the
character from `code` when the stored bytes are exactly that truncated tail, and
`build` writes `84 A2` back, so the round trip stays lossless.

## Round-trip notes

- Strings are written one copy per attribute, like the shipped files. Pass
  `--dedup` to reuse identical strings instead.
- Attribute order is preserved, so a file that is extracted and rebuilt without
  edits keeps its attribute and pool layout.
- The shipped Obscure 2 files contain one attribute record that no node
  references (a dead entry). It is not reproduced, so a rebuilt file is a few
  bytes smaller. Final Exam has the same dead record in 208 of its 382 files,
  which makes those rebuilt files exactly 8 bytes shorter.
- Control characters that XML 1.0 cannot represent are written as `\u0012`
  escapes and restored on build.
- Final Exam string pools are zero-padded to a 16-byte boundary and the padding
  is encrypted along with the strings, so the pool keeps its original size. The
  node ordering can differ, because the shipped files do not follow a single
  traversal order and the game addresses nodes by index. All 382 shipped Final
  Exam files keep an identical XML tree.

