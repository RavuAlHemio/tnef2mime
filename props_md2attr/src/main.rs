use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::collections::hash_map::Entry as HashMapEntry;
use std::env;
use std::ffi::OsString;
use std::fs::{File, read_dir};
use std::io::Read;
use std::mem::replace;
use std::path::{Path, PathBuf};

use docx2attr_common::docx_to_paragraphs;
use once_cell::sync::Lazy;
use regex::{Regex, RegexSet};


const PROPERTY_PREFIX: &str = "Pid";
static MARKDOWN_NAME_RE: Lazy<Regex> = Lazy::new(|| Regex::new(concat!(
    "(?m)",
    "^",
    "\\s*",
    "#",
    "\\s*",
    "(?P<value>[A-Za-z0-9_]+)",
    "(?:",
        "\\s+",
        "Cann?onical",
        "\\s+",
        "Property",
    ")?",
    "\\s*",
    "$",
)).unwrap());
static MARKDOWN_VALUE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(concat!(
    "(?m)",
    "^",
    "\\s*",
    "\\|",
    "\\s*",
    "(?:",
        "Identifier:",
        "|",
        "Long ID \\(LID\\):",
    ")",
    "\\s*",
    "(?:<br\\s*/>\\s*)?",
    "\\|",
    "\\s*",
    "0x",
    "(?P<value>[0-9A-Fa-f]+)",
    "\\s*",
    "(?:<br\\s*/>\\s*)?",
    "\\|",
    "\\s*",
    "$",
)).unwrap());
static DOCX_NAME_RE: Lazy<Regex> = Lazy::new(|| Regex::new(concat!(
    "^",
    "\\s*",
    "Canonical name:",
    "\\s*",
    "(?P<value>[A-Za-z0-9_]+)",
    "\\s*",
    "$",
)).unwrap());
static DOCX_VALUE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(concat!(
    "^",
    "\\s*",
    "(?:",
        "Property ID:",
        "|",
        "Property long ID \\(LID\\):",
    ")",
    "\\s*",
    "0x(?P<value>[0-9A-Fa-f]+)",
    "\\s*",
    "$",
)).unwrap());
static DOCX_RE_SET: Lazy<RegexSet> = Lazy::new(|| RegexSet::new(&[
    DOCX_NAME_RE.as_str(),
    DOCX_VALUE_RE.as_str(),
]).unwrap());


struct PropertyCollection {
    pub properties: Vec<Property>,
    pub known_value_to_name: HashMap<u16, String>,
    pub known_names: HashSet<String>,
}
impl PropertyCollection {
    pub fn new() -> Self {
        Self {
            properties: Vec::new(),
            known_value_to_name: HashMap::new(),
            known_names: HashSet::new(),
        }
    }

    pub fn add_property(&mut self, mut key: String, value: u16) {
        // properties may not start with number
        if key.chars().nth(0).map(|c| c.is_ascii_digit()).unwrap_or(false) {
            key.insert(0, '_');
        }

        if !self.known_names.insert(key.clone()) {
            // we already have a variant by this name
            return;
        }

        match self.known_value_to_name.entry(value) {
            HashMapEntry::Occupied(o) => {
                self.properties.push(Property::Aliased(AliasedProperty {
                    name: key,
                    target: o.get().clone(),
                }));
            },
            HashMapEntry::Vacant(v) => {
                v.insert(key.clone());
                self.properties.push(Property::Defined(DefinedProperty {
                    name: key,
                    value,
                }));
            },
        }
    }
}


#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum Property {
    Defined(DefinedProperty),
    Aliased(AliasedProperty),
}
impl Property {
    pub fn to_enum_variant(&self) -> String {
        match self {
            Self::Defined(d) => d.to_enum_variant(),
            Self::Aliased(a) => a.to_enum_variant(),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct DefinedProperty {
    pub value: u16,
    pub name: String,
}
impl DefinedProperty {
    pub fn to_enum_variant(&self) -> String {
        format!("    {} = 0x{:04X},", self.name, self.value)
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct AliasedProperty {
    pub name: String,
    pub target: String,
}
impl AliasedProperty {
    pub fn to_enum_variant(&self) -> String {
        format!("    // {} = {}", self.name, self.target)
    }
}


fn add_markdown_properties(markdown_path: &Path, properties: &mut PropertyCollection) {
    let entries = read_dir(&markdown_path)
        .expect("failed to read directory");
    for entry_res in entries {
        let entry = entry_res.expect("failed to get directory entry");
        let file_name = entry.file_name();
        let utf8_file_name = match file_name.to_str() {
            Some(u8fn) => u8fn,
            None => continue,
        };

        if !utf8_file_name.to_lowercase().starts_with("pidtag") {
            continue;
        }

        let mut buf = Vec::new();
        let mut file = match File::open(entry.path()) {
            Ok(f) => f,
            Err(e) => panic!("failed to open file {}: {}", utf8_file_name, e),
        };
        if let Err(e) = file.read_to_end(&mut buf) {
            panic!("failed to read file {}: {}", utf8_file_name, e);
        }

        let string = match String::from_utf8(buf) {
            Ok(s) => s.replace("\r", ""),
            Err(e) => panic!("failed to decode file {} as UTF-8: {}", utf8_file_name, e),
        };
        let name = match MARKDOWN_NAME_RE.captures(&string) {
            Some(c) => c.name("value").unwrap().as_str(),
            None => panic!("failed to find name in {}", utf8_file_name),
        };
        let stripped_name = match name.strip_prefix(PROPERTY_PREFIX) {
            Some(p) => p.to_owned(),
            None => continue,
        };
        let value_str = match MARKDOWN_VALUE_RE.captures(&string) {
            Some(c) => c.name("value").unwrap().as_str(),
            None => {
                eprintln!("failed to find value in {}; skipping", utf8_file_name);
                continue;
            },
        };
        let value: u16 = match u16::from_str_radix(value_str, 16) {
            Ok(v) => v,
            Err(_) => {
                eprintln!("value 0x{} for {} does not fit u16; skipping", value_str, name);
                continue;
            },
        };

        properties.add_property(stripped_name, value);
    }
}


fn add_docx_properties(docx_path: &Path, properties: &mut PropertyCollection) {
    let paragraphs = docx_to_paragraphs(
        &docx_path,
        |para| DOCX_RE_SET.is_match(para),
    );

    let mut name: Option<String> = None;
    let mut value: Option<u16> = None;
    for paragraph in &paragraphs {
        if let Some(caps) = DOCX_NAME_RE.captures(paragraph) {
            if let Some(n) = &name {
                if let Some(v) = value {
                    let new_name = replace(&mut name, None).unwrap();
                    properties.add_property(new_name, v);
                } else {
                    eprintln!("docx property {} does not have a value; skipping", n);
                }
            }

            let name_str = caps.name("value").unwrap().as_str();
            name = if let Some(stripped) = name_str.strip_prefix(PROPERTY_PREFIX) {
                Some(stripped.to_owned())
            } else {
                None
            };
        } else if let Some(caps) = DOCX_VALUE_RE.captures(paragraph) {
            let value_str = caps.name("value").unwrap().as_str();
            let new_value = match u16::from_str_radix(value_str, 16) {
                Ok(nv) => nv,
                Err(_) => {
                    eprintln!("failed to parse {} as u16 as value for {:?}", value_str, name);
                    continue;
                },
            };
            value = Some(new_value);
        }
    }

    if name.is_some() {
        if let Some(v) = value {
            let new_name = replace(&mut name, None).unwrap();
            properties.add_property(new_name, v);
        }
    }
}


fn run() -> i32 {
    let args: Vec<OsString> = env::args_os().collect();
    if args.len() != 3 {
        let prog_name = args.get(0)
            .map(|a| a.to_string_lossy())
            .unwrap_or(Cow::Borrowed("mapi_docx2attr"));
        eprintln!("Usage: {} MAPI_DOC_DIR MS-OXPROPS.DOCX", prog_name);
        return 1;
    }

    let markdown_path = PathBuf::from(&args[1]);
    let docx_path = PathBuf::from(&args[2]);

    let mut properties = PropertyCollection::new();

    // DOCX trumps Markdown
    add_docx_properties(&docx_path, &mut properties);
    add_markdown_properties(&markdown_path, &mut properties);

    properties.properties.sort_unstable();

    println!("// This file has been generated by props_md2attr.");
    println!();
    println!("use from_to_repr::from_to_other;");
    println!();
    println!();
    println!("#[derive(Clone, Copy, Debug)]");
    println!("#[from_to_other(base_type = u16, derive_compare = \"as_int\")]");
    println!("pub enum PropTag {{");
    for property in &properties.properties {
        println!("{}", property.to_enum_variant());
    }
    println!("    Other(u16),");
    println!("}}");

    0
}


fn main() {
    std::process::exit(run());
}
