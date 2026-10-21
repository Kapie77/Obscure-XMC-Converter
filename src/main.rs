use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use crc32fast::Hasher;
use serde_json::Value;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};
use xmltree::{Element, EmitterConfig, XMLNode};

const NODE_SIZE: usize = 16;
const ATTR_SIZE: usize = 8;

#[derive(Parser, Debug)]
#[command(
    name = "obscure-xmc-converter",
    version,
    about = "Convert between Obscure 2 XMC and XML",
    long_about = "Extract XMC files to XML or build XMC files from XML."
)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Extract an XMC file to XML
    Extract {
        /// Input XMC files
        #[arg(required = true, value_name = "INPUT")]
        inputs: Vec<PathBuf>,

        /// Output file for one input, or output directory for multiple inputs
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// CRC dictionary JSON or text file
        #[arg(long = "dict")]
        dict_file: Option<PathBuf>,

        /// Print additional binary information
        #[arg(long)]
        debug: bool,
    },

    /// Build an XMC file from XML
    Build {
        /// Input XML files
        #[arg(required = true, value_name = "INPUT")]
        inputs: Vec<PathBuf>,

        /// Output file for one input, or output directory for multiple inputs
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// CRC dictionary JSON or text file
        #[arg(long = "dict")]
        dict_file: Option<PathBuf>,

        /// Print additional binary information
        #[arg(long)]
        debug: bool,
    },
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Mode {
    Xmc2xml,
    Xml2xmc,
}

#[derive(Clone, Debug)]
struct XmcNode {
    tag_crc: u32,
    parent_idx: i16,
    child_count: i16,
    first_child: i16,
    attr_count: i16,
    first_attr: i16,
}

#[derive(Clone, Debug)]
struct XmcAttribute {
    name_crc: u32,
    value_offset: u32,
}

fn read_u32(data: &[u8], offset: usize) -> Result<u32> {
    let bytes = data.get(offset..offset + 4).context("Unexpected end of file reading u32")?;
    Ok(u32::from_le_bytes(bytes.try_into()?))
}

fn read_i16(data: &[u8], offset: usize) -> Result<i16> {
    let bytes = data.get(offset..offset + 2).context("Unexpected end of file reading i16")?;
    Ok(i16::from_le_bytes(bytes.try_into()?))
}

fn parse_xmc(data: &[u8]) -> Result<(Vec<XmcNode>, Vec<XmcAttribute>, Vec<u8>)> {
    if data.len() < 12 {
        bail!("File is too small to contain an XMC header");
    }
    let node_count = read_u32(data, 0)? as usize;
    let attr_count = read_u32(data, 4)? as usize;
    let string_size = read_u32(data, 8)? as usize;

    let node_bytes = node_count.checked_mul(NODE_SIZE).context("Node count overflow")?;
    let attr_bytes = attr_count.checked_mul(ATTR_SIZE).context("Attribute count overflow")?;
    let attr_start = 12usize.checked_add(node_bytes).context("File offset overflow")?;
    let strings_start = attr_start.checked_add(attr_bytes).context("File offset overflow")?;
    let end = strings_start.checked_add(string_size).context("File size overflow")?;
    if end > data.len() {
        bail!("XMC header counts exceed file size (expected at least {end} bytes, got {})", data.len());
    }

    let mut nodes = Vec::with_capacity(node_count);
    for i in 0..node_count {
        let o = 12 + i * NODE_SIZE;
        nodes.push(XmcNode {
            tag_crc: read_u32(data, o)?,
            parent_idx: read_i16(data, o + 4)?,
            child_count: read_i16(data, o + 6)?,
            first_child: read_i16(data, o + 8)?,
            attr_count: read_i16(data, o + 10)?,
            first_attr: read_i16(data, o + 12)?,
        });
    }

    let mut attrs = Vec::with_capacity(attr_count);
    for i in 0..attr_count {
        let o = attr_start + i * ATTR_SIZE;
        attrs.push(XmcAttribute {
            name_crc: read_u32(data, o)?,
            value_offset: read_u32(data, o + 4)?,
        });
    }
    Ok((nodes, attrs, data[strings_start..end].to_vec()))
}

fn crc32(text: &str) -> u32 {
    let mut h = Hasher::new();
    h.update(text.as_bytes());
    h.finalize()
}

fn load_crc_dictionary(path: &Path) -> Result<HashMap<u32, String>> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Could not read dictionary {}", path.display()))?;
    let mut map = HashMap::new();

    match serde_json::from_str::<Value>(&content) {
        Ok(Value::Object(obj)) => {
            for (key, value) in obj {
                let name = match value.as_str() {
                    Some(s) => s.to_owned(),
                    None => continue,
                };
                let crc = parse_crc_key(&key).unwrap_or_else(|| crc32(&key));
                map.insert(crc, name);
            }
        }
        Ok(Value::Array(items)) => {
            for item in items {
                if let Some(name) = item.as_str() {
                    map.insert(crc32(name), name.to_owned());
                }
            }
        }
        _ => {
            for line in content.lines().map(str::trim).filter(|s| !s.is_empty()) {
                map.insert(crc32(line), line.to_owned());
            }
        }
    }
    println!("Loaded {} CRC dictionary entries", map.len());
    Ok(map)
}

fn parse_crc_key(key: &str) -> Option<u32> {
    if let Some(hex) = key.strip_prefix("0x").or_else(|| key.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16).ok()
    } else {
        key.parse::<u32>().ok()
    }
}

fn resolve_name(crc: u32, dict: &HashMap<u32, String>, prefix: &str) -> String {
    dict.get(&crc).cloned().unwrap_or_else(|| format!("{prefix}_{crc:08X}"))
}

fn pool_string(pool: &[u8], offset: u32) -> String {
    let start = offset as usize;
    if start >= pool.len() {
        return String::new();
    }
    let end = pool[start..].iter().position(|b| *b == 0)
        .map(|n| start + n).unwrap_or(pool.len());
    String::from_utf8_lossy(&pool[start..end]).into_owned()
}

fn element_from_node(
    idx: usize,
    nodes: &[XmcNode],
    attrs: &[XmcAttribute],
    pool: &[u8],
    dict: &HashMap<u32, String>,
    depth: usize,
) -> Result<Element> {
    if depth > nodes.len() {
        bail!("Possible circular reference in XMC node tree");
    }
    let node = nodes.get(idx).context("Node index out of range")?;
    let mut elem = Element::new(&resolve_name(node.tag_crc, dict, "tag"));

    if node.attr_count > 0 {
        if node.first_attr < 0 {
            bail!("Node {idx} has attributes but an invalid first_attr");
        }
        for n in 0..node.attr_count as usize {
            let ai = node.first_attr as usize + n;
            let attr = attrs.get(ai).with_context(|| format!("Attribute index {ai} out of range"))?;
            elem.attributes.insert(
                resolve_name(attr.name_crc, dict, "attr"),
                pool_string(pool, attr.value_offset),
            );
        }
    }

    if node.child_count > 0 {
        if node.first_child < 0 {
            bail!("Node {idx} has children but an invalid first_child");
        }
        for n in 0..node.child_count as usize {
            let child_idx = node.first_child as usize + n;
            elem.children.push(XMLNode::Element(element_from_node(
                child_idx, nodes, attrs, pool, dict, depth + 1,
            )?));
        }
    }
    Ok(elem)
}

fn xmc_to_xml(input: &Path, output: &Path, dict: &HashMap<u32, String>, debug: bool) -> Result<()> {
    let data = fs::read(input).with_context(|| format!("Could not read {}", input.display()))?;
    let (nodes, attrs, pool) = parse_xmc(&data)?;
    if debug {
        println!("Header: nodes={}, attrs={}, strings={} bytes", nodes.len(), attrs.len(), pool.len());
        for (i, node) in nodes.iter().take(10).enumerate() {
            println!("node[{i}]: {node:?}");
        }
    }

    let mut xml = Vec::new();
    xml.extend_from_slice(b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    if !nodes.is_empty() {
        let root = element_from_node(0, &nodes, &attrs, &pool, dict, 0)?;
        root.write_with_config(
            &mut xml,
            EmitterConfig::new().perform_indent(true).write_document_declaration(false),
        )?;
        xml.push(b'\n');
    }
    fs::write(output, xml).with_context(|| format!("Could not write {}", output.display()))?;
    println!("Converted XMC -> XML: {}", output.display());
    Ok(())
}

fn name_to_crc(name: &str, reverse: &HashMap<String, u32>) -> u32 {
    if let Some(crc) = reverse.get(name) {
        return *crc;
    }
    for prefix in ["tag_", "attr_"] {
        if let Some(hex) = name.strip_prefix(prefix) {
            if hex.len() == 8 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
                if let Ok(value) = u32::from_str_radix(hex, 16) {
                    return value;
                }
            }
        }
    }
    crc32(name)
}

fn validate_i16(value: usize, field: &str) -> Result<i16> {
    if value > i16::MAX as usize {
        bail!("{field} exceeds the supported i16 range: {value}");
    }
    Ok(value as i16)
}

fn build_element(
    element: &Element,
    parent: i16,
    nodes: &mut Vec<XmcNode>,
    attrs: &mut Vec<XmcAttribute>,
    pool: &mut Vec<u8>,
    string_offsets: &mut HashMap<String, u32>,
    reverse: &HashMap<String, u32>,
) -> Result<usize> {
    let idx = nodes.len();
    nodes.push(XmcNode {
        tag_crc: name_to_crc(&element.name, reverse),
        parent_idx: parent,
        child_count: 0,
        first_child: -1,
        attr_count: 0,
        first_attr: -1,
    });

    let first_attr = attrs.len();
    for (name, value) in &element.attributes {
        let offset = if let Some(offset) = string_offsets.get(value) {
            *offset
        } else {
            let offset = u32::try_from(pool.len()).context("String pool exceeds u32 range")?;
            pool.extend_from_slice(value.as_bytes());
            pool.push(0);
            string_offsets.insert(value.clone(), offset);
            offset
        };
        attrs.push(XmcAttribute { name_crc: name_to_crc(name, reverse), value_offset: offset });
    }
    let attr_count = element.attributes.len();
    nodes[idx].attr_count = validate_i16(attr_count, "attribute count")?;
    if attr_count > 0 {
        nodes[idx].first_attr = validate_i16(first_attr, "first attribute index")?;
    }

    let child_elements: Vec<&Element> = element.children.iter().filter_map(|n| {
        if let XMLNode::Element(e) = n { Some(e) } else { None }
    }).collect();

    let mut child_indices = Vec::with_capacity(child_elements.len());
    for child in &child_elements {
        // Reserve all siblings first so child indices stay contiguous, as XMC expects.
        let child_idx = nodes.len();
        nodes.push(XmcNode {
            tag_crc: name_to_crc(&child.name, reverse),
            parent_idx: validate_i16(idx, "parent index")?,
            child_count: 0,
            first_child: -1,
            attr_count: 0,
            first_attr: -1,
        });
        child_indices.push(child_idx);
    }
    nodes[idx].child_count = validate_i16(child_indices.len(), "child count")?;
    if let Some(first) = child_indices.first() {
        nodes[idx].first_child = validate_i16(*first, "first child index")?;
    }

    for (child, child_idx) in child_elements.iter().zip(child_indices.iter()) {
        // Build attributes and descendants for the already-reserved node.
        populate_reserved_child(
            child, *child_idx, nodes, attrs, pool, string_offsets, reverse,
        )?;
    }
    Ok(idx)
}

fn populate_reserved_child(
    element: &Element,
    idx: usize,
    nodes: &mut Vec<XmcNode>,
    attrs: &mut Vec<XmcAttribute>,
    pool: &mut Vec<u8>,
    string_offsets: &mut HashMap<String, u32>,
    reverse: &HashMap<String, u32>,
) -> Result<()> {
    let parent = nodes[idx].parent_idx;
    // Build this node's attributes and children in a helper, without adding another root node.
    let first_attr = attrs.len();
    for (name, value) in &element.attributes {
        let offset = if let Some(offset) = string_offsets.get(value) {
            *offset
        } else {
            let offset = u32::try_from(pool.len()).context("String pool exceeds u32 range")?;
            pool.extend_from_slice(value.as_bytes());
            pool.push(0);
            string_offsets.insert(value.clone(), offset);
            offset
        };
        attrs.push(XmcAttribute { name_crc: name_to_crc(name, reverse), value_offset: offset });
    }
    nodes[idx].attr_count = validate_i16(element.attributes.len(), "attribute count")?;
    if !element.attributes.is_empty() {
        nodes[idx].first_attr = validate_i16(first_attr, "first attribute index")?;
    }

    let children: Vec<&Element> = element.children.iter().filter_map(|n| {
        if let XMLNode::Element(e) = n { Some(e) } else { None }
    }).collect();
    let mut indices = Vec::new();
    for child in &children {
        let child_idx = nodes.len();
        nodes.push(XmcNode {
            tag_crc: name_to_crc(&child.name, reverse),
            parent_idx: validate_i16(idx, "parent index")?,
            child_count: 0,
            first_child: -1,
            attr_count: 0,
            first_attr: -1,
        });
        indices.push(child_idx);
    }
    nodes[idx].child_count = validate_i16(indices.len(), "child count")?;
    if let Some(first) = indices.first() {
        nodes[idx].first_child = validate_i16(*first, "first child index")?;
    }
    for (child, child_idx) in children.iter().zip(indices.iter()) {
        populate_reserved_child(child, *child_idx, nodes, attrs, pool, string_offsets, reverse)?;
    }
    let _ = parent;
    Ok(())
}

fn xml_to_xmc(input: &Path, output: &Path, dict: &HashMap<u32, String>, debug: bool) -> Result<()> {
    let file = fs::File::open(input).with_context(|| format!("Could not open {}", input.display()))?;
    let root = Element::parse(file).with_context(|| format!("Could not parse XML {}", input.display()))?;

    let mut reverse = HashMap::new();
    for (crc, name) in dict {
        reverse.insert(name.clone(), *crc);
    }

    let mut nodes = Vec::new();
    let mut attrs = Vec::new();
    let mut pool = Vec::new();
    let mut string_offsets = HashMap::new();

    build_element(
        &root, -1, &mut nodes, &mut attrs, &mut pool, &mut string_offsets, &reverse,
    )?;

    let node_count = u32::try_from(nodes.len()).context("Too many nodes")?;
    let attr_count = u32::try_from(attrs.len()).context("Too many attributes")?;
    let pool_size = u32::try_from(pool.len()).context("String pool too large")?;

    let mut output_bytes = Vec::with_capacity(12 + nodes.len() * NODE_SIZE + attrs.len() * ATTR_SIZE + pool.len());
    output_bytes.extend_from_slice(&node_count.to_le_bytes());
    output_bytes.extend_from_slice(&attr_count.to_le_bytes());
    output_bytes.extend_from_slice(&pool_size.to_le_bytes());

    for node in &nodes {
        output_bytes.extend_from_slice(&node.tag_crc.to_le_bytes());
        output_bytes.extend_from_slice(&node.parent_idx.to_le_bytes());
        output_bytes.extend_from_slice(&node.child_count.to_le_bytes());
        output_bytes.extend_from_slice(&node.first_child.to_le_bytes());
        output_bytes.extend_from_slice(&node.attr_count.to_le_bytes());
        output_bytes.extend_from_slice(&node.first_attr.to_le_bytes());
        output_bytes.extend_from_slice(&0u16.to_le_bytes());
    }
    for attr in &attrs {
        output_bytes.extend_from_slice(&attr.name_crc.to_le_bytes());
        output_bytes.extend_from_slice(&attr.value_offset.to_le_bytes());
    }
    output_bytes.extend_from_slice(&pool);

    if debug {
        println!("Debug: nodes={}, attrs={}, string_pool={} bytes", nodes.len(), attrs.len(), pool.len());
    }
    fs::write(output, &output_bytes).with_context(|| format!("Could not write {}", output.display()))?;
    println!("Converted XML -> XMC: {} ({} bytes)", output.display(), output_bytes.len());
    Ok(())
}

fn resolve_default_dict() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;
    let candidates = [
        exe_dir.join("hashes.json"),
        exe_dir.join("utils").join("hashes.json"),
        exe_dir.join("zwoLib").join("utils").join("hashes.json"),
        PathBuf::from("hashes.json"),
    ];
    candidates.into_iter().find(|p| p.exists())
}

fn output_path(source: &Path, mode: Mode, override_path: Option<&Path>, batch: bool) -> PathBuf {
    let extension = if mode == Mode::Xmc2xml { "xml" } else { "xmc" };
    let Some(out) = override_path else {
        return source.with_extension(extension);
    };
    if batch || out.is_dir() || out.extension().is_none() {
        let _ = fs::create_dir_all(out);
        return out.join(source.file_stem().unwrap_or_default()).with_extension(extension);
    }
    out.to_path_buf()
}


fn parse_args_with_implicit_mode() -> Args {
    use std::ffi::OsString;

    let raw: Vec<OsString> = std::env::args_os().collect();

    let first = raw.get(1).and_then(|arg| arg.to_str());

    // Preserve explicit commands and normal help/version requests.
    if matches!(
        first,
        Some("extract" | "build" | "--help" | "-h" | "--version" | "-V")
    ) {
        return Args::parse_from(raw);
    }

    let mut xmc_count = 0usize;
    let mut xml_count = 0usize;
    let mut skip_next = false;

    for arg in raw.iter().skip(1) {
        let Some(value) = arg.to_str() else {
            continue;
        };

        // These options take a value that must not be treated as input.
        if skip_next {
            skip_next = false;
            continue;
        }

        if matches!(value, "--dict" | "-o" | "--output") {
            skip_next = true;
            continue;
        }

        // Also support --dict=... and --output=...
        if value.starts_with("--dict=")
            || value.starts_with("--output=")
            || value.starts_with("-o=")
        {
            continue;
        }

        // Ignore other command-line options.
        if value.starts_with('-') {
            continue;
        }

        let extension = std::path::Path::new(value)
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        match extension.as_str() {
            "xmc" => xmc_count += 1,
            "xml" => xml_count += 1,
            _ => {}
        }
    }

    if xmc_count == 0 && xml_count == 0 {
        eprintln!(
            "No .xmc or .xml input detected. \
             Use 'extract' for XMC files or 'build' for XML files."
        );
        std::process::exit(2);
    }

    // Preserve the old automatic behavior: XMC wins a tie.
    let command = if xml_count > xmc_count {
        "build"
    } else {
        "extract"
    };

    let mut forwarded = Vec::with_capacity(raw.len() + 1);
    forwarded.push(raw[0].clone());
    forwarded.push(command.into());
    forwarded.extend(raw.into_iter().skip(1));

    Args::parse_from(forwarded)
}

fn main() -> Result<()> {
    let args = parse_args_with_implicit_mode();

    let (inputs, output, dict_file, debug, mode) = match args.command {
        Command::Extract {
            inputs,
            output,
            dict_file,
            debug,
        } => (inputs, output, dict_file, debug, Mode::Xmc2xml),

        Command::Build {
            inputs,
            output,
            dict_file,
            debug,
        } => (inputs, output, dict_file, debug, Mode::Xml2xmc),
    };

    let dict_path = dict_file.or_else(resolve_default_dict);

    let dict = match dict_path {
        Some(path) => load_crc_dictionary(&path)
            .with_context(|| format!("Failed to load dictionary {}", path.display()))?,
        None => {
            eprintln!(
                "Warning: hashes.json not found; unknown names will use CRC32."
            );
            HashMap::new()
        }
    };

    let expected_extension = match mode {
        Mode::Xmc2xml => "xmc",
        Mode::Xml2xmc => "xml",
    };

    let mut sources = Vec::new();
    let mut ignored = Vec::new();
    let mut missing = Vec::new();

    for path in &inputs {
        if !path.exists() {
            missing.push(path.clone());
            continue;
        }

        let extension = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        if extension == expected_extension {
            sources.push(path.clone());
        } else {
            ignored.push(path.clone());
        }
    }

    for path in missing {
        eprintln!("Skipping missing file: {}", path.display());
    }

    for path in ignored {
        eprintln!(
            "Ignoring file with unsupported extension for this command: {}",
            path.display()
        );
    }

    if sources.is_empty() {
        bail!(
            "No compatible input files found. Expected .{expected_extension} files."
        );
    }

    let batch = sources.len() > 1;

    for source in &sources {
        let destination = output_path(
            source,
            mode,
            output.as_deref(),
            batch,
        );

        match mode {
            Mode::Xmc2xml => {
                xmc_to_xml(source, &destination, &dict, debug)?;
            }
            Mode::Xml2xmc => {
                xml_to_xmc(source, &destination, &dict, debug)?;
            }
        }
    }

    if batch {
        println!(
            "Processed {} files in mode {:?}",
            sources.len(),
            mode
        );
    }

    Ok(())
}