mod binread;
mod guid;
mod tnef;


use std::borrow::Cow;
use std::env;
use std::ffi::OsString;
use std::fs::File;
use std::io::{Cursor, Read, Write};

use codepage::to_encoding;
use encoding_rs::{Encoding, UTF_8};
use env_logger;

use crate::tnef::{decode_properties, PropTag, PropValue, read_tnef, TnefAttributeId};


fn hexdump(bytes: &[u8], prefix: &str) {
    let mut i = 0;

    while i < bytes.len() {
        print!("{}{:08x}", prefix, i);
        for j in 0..16 {
            if i + j < bytes.len() {
                print!(" {:02x}", bytes[i + j]);
            } else {
                print!("   ");
            }
            if j == 7 {
                print!(" ");
            }
        }
        print!(" |");
        for j in 0..16 {
            if i + j < bytes.len() {
                let b = bytes[i + j];
                if (b >= 0x20 && b <= 0x7E) || b >= 0xA0 {
                    let c = char::from_u32(b.into()).unwrap();
                    print!("{}", c);
                } else {
                    print!(".");
                }
            }
        }
        println!("|");

        i += 16;
    }
}


fn run() -> i32 {
    let args: Vec<OsString> = env::args_os().collect();
    if args.len() != 2 {
        let arg0 = args
            .get(0)
            .map(|a| a.to_string_lossy())
            .unwrap_or(Cow::Borrowed("tnef2mime"));
        eprintln!("Usage: {} MESSAGE", arg0);
        return 1;
    }

    env_logger::init();

    let mut buf = Vec::new();
    {
        let mut file = File::open(&args[1])
            .expect("failed to open file");
        file.read_to_end(&mut buf)
            .expect("failed to read file");
    }

    let mut encoder: &Encoding = UTF_8;

    let mut headers = None;
    let mut body = None;

    let buf_cursor = Cursor::new(&buf);
    let tnef = read_tnef(buf_cursor)
        .expect("failed to read TNEF");
    println!("legacy key: {}", tnef.legacy_key);
    for attribute in &tnef.attributes {
        println!("attribute {:?}.{:?}", attribute.level, attribute.id);
        if attribute.id == TnefAttributeId::OemCodepage && attribute.data.len() >= 2 {
            let codepage_id =
                ((attribute.data[0] as u16) << 0)
                | ((attribute.data[1] as u16) << 8)
            ;
            if let Some(new_encoder) = to_encoding(codepage_id) {
                encoder = new_encoder;
            }
        } else if attribute.id == TnefAttributeId::MsgProps || attribute.id == TnefAttributeId::Attachment {
            match decode_properties(Cursor::new(&attribute.data), encoder) {
                Ok(props) => {
                    for prop in &props {
                        if prop.tag == PropTag::TagAttachDataBinary {
                            if let PropValue::Object(val) = &prop.value {
                                let mut attachment = File::create("attachment.bin")
                                    .expect("failed to open attachment.bin");
                                attachment.write_all(&val[16..])
                                    .expect("failed to write attachment.bin");
                            }
                        } else if prop.tag == PropTag::TagTransportMessageHeaders {
                            if let PropValue::String8(msg_headers) = &prop.value {
                                headers = Some(msg_headers.trim_end_matches('\0').to_owned());
                            }
                        } else if prop.tag == PropTag::TagBodyHtml {
                            if let PropValue::Binary(msg_body) = &prop.value {
                                body = Some(msg_body.clone());
                            }
                        }
                        println!("    {:?}: {:?}", prop.tag, prop.value);
                    }
                },
                Err(e) => {
                    println!("    failed to decode properties: {}", e);
                    hexdump(&attribute.data, "    ");
                    continue;
                },
            };
        } else {
            hexdump(&attribute.data, "    ");
        }
    }

    if let Some(h) = headers {
        if let Some(b) = body {
            let mut email = File::create("email.eml")
                .expect("failed to open email.eml");
            email.write_all(h.as_bytes())
                .expect("failed to write email.eml headers");
            email.write_all(&b)
                .expect("failed to write email.eml body");
        }
    }

    0
}


fn main() {
    std::process::exit(run());
}
