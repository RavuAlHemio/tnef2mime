use std::fs::File;
use std::io::Read;
use std::mem::replace;
use std::path::Path;

use quick_xml;
use quick_xml::events::Event as XmlEvent;
use quick_xml::name::ResolveResult;
use zip::ZipArchive;


const WORD_NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";


fn resolve_namespace<'a>(namespace: ResolveResult<'a>) -> Option<String> {
    match namespace {
        ResolveResult::Bound(b) => Some(String::from_utf8_lossy(b.0).into_owned()),
        ResolveResult::Unbound => None,
        ResolveResult::Unknown(_) => None,
    }
}

pub fn docx_to_paragraphs<P: FnMut(&String) -> bool>(path: &Path, mut paragraph_predicate: P) -> Vec<String> {
    let body_string = {
        // open DOCX file
        let docx_file = File::open(path)
            .expect("failed to open docx file");
        let mut docx_zip = ZipArchive::new(docx_file)
            .expect("failed to read docx file");

        // read document body
        let mut docx_body_file = docx_zip.by_name("word/document.xml")
            .expect("failed to open word/document.xml from docx file");
        let mut body_bytes = Vec::new();
        docx_body_file.read_to_end(&mut body_bytes)
            .expect("failed to read word/document.xml from docx file");
        String::from_utf8(body_bytes)
            .expect("failed to decode word/document.xml from docx file as UTF-8")
    };

    // parse DOCX as XML
    let mut parser = quick_xml::NsReader::from_str(&body_string);
    let mut buf = Vec::new();
    let mut name_stack = Vec::new();
    let mut ret = Vec::new();
    let mut current_text = String::new();
    let mut collect_text = false;
    loop {
        match parser.read_resolved_event_into(&mut buf) {
            Ok((_, XmlEvent::Eof)) => break,
            Ok((ns, XmlEvent::Start(start))) => {
                let ns_str = resolve_namespace(ns);
                let name_str = String::from_utf8_lossy(start.name().local_name().into_inner()).into_owned();
                if ns_str.as_ref().map(|ns| ns == WORD_NS).unwrap_or(false) {
                    if name_str == "p" {
                        // paragraph; clear out text
                        current_text.clear();
                    } else if name_str == "t" {
                        // text started; begin collecting it
                        collect_text = true;
                    }
                }
                name_stack.push((ns_str, name_str));
            },
            Ok((_ns, XmlEvent::End(_end))) => {
                let (ns_str, name_str) = name_stack.pop().unwrap();
                if ns_str.as_ref().map(|ns| ns == WORD_NS).unwrap_or(false) {
                    if name_str == "p" {
                        // paragraph ended; store collected text
                        let paragraph = replace(&mut current_text, String::new());
                        if paragraph_predicate(&paragraph) {
                            ret.push(paragraph);
                        }
                    } else if name_str == "t" {
                        // text ended; stop collecting
                        collect_text = false;
                    }
                }
            },
            Ok((_ns, XmlEvent::Text(txt))) => {
                if collect_text {
                    current_text.push_str(txt.unescape().unwrap().as_ref());
                }
            },
            Ok(_) => {},
            Err(e) => panic!("error parsing docx: {}", e),
        }
    }
    ret
}


pub fn byte_string_to_le_int_string(byte_str: &str) -> String {
    let mut pieces: Vec<&str> = byte_str.split('.').collect();
    pieces.reverse();
    pieces.concat()
}
