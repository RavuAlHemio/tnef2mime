use std::ffi::OsString;
use std::fs::File;
use std::io::{Cursor, Read, Seek};
use std::process::ExitCode;

use codepage;
use encoding_rs::DecoderResult;
use from_to_repr::FromToRepr;
use msox::{BinaryReader, PropType, PropValue};
use uuid::Uuid;


#[derive(Clone, Copy, Debug, FromToRepr, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u32)]
enum Marker {
    // Folders
    StartTopFld = 0x4009_0003,
    StartSubFld = 0x400A_0003,
    EndFolder = 0x400B_0003,
    // Messages and their parts
    StartMessage = 0x400C_0003,
    EndMessage = 0x400D_0003,
    StartFAIMsg = 0x4010_0003,
    StartEmbed = 0x4001_0003,
    EndEmbed = 0x4002_0003,
    StartRecip = 0x4003_0003,
    EndToRecip = 0x4004_0003,
    NewAttach = 0x4000_0003,
    EndAttach = 0x400E_0003,
    // Synchronization download
    IncrSyncChg = 0x4012_0003,
    IncrSyncChgPartial = 0x407D_0003,
    IncrSyncDel = 0x4013_0003,
    IncrSyncEnd = 0x4014_0003,
    IncrSyncRead = 0x402F_0003,
    IncrSyncStateBegin = 0x403A_0003,
    IncrSyncStateEnd = 0x403B_0003,
    IncrSyncProgressMode = 0x4074_000B,
    IncrSyncProgressPerMsg = 0x4075_000B,
    IncrSyncMessage = 0x4015_0003,
    IncrSyncGroupInfo = 0x407B_0102,
    // Special
    FXErrorInfo = 0x4018_0003,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum PropertyId {
    Tagged { tag: u16 },
    Named { property_set: Uuid, name_info: PropertyNameInfo },
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum PropertyNameInfo {
    DisplayId(u32),
    Name(String),
}


fn parse_message<R: Read + Seek>(file: &mut R) {
    loop {
        println!("we are at {}", file.stream_position().unwrap());
        let Some(marker_or_prop) = file.read_u32_le_or_eof()
            .expect("failed to read marker/property") else { break };
        if let Some(marker_opt) = Marker::try_from_repr(marker_or_prop) {
            println!("marker: {:?}", marker_opt);
            continue;
        }
        println!("MOP: {:#010X}", marker_or_prop);

        let prop_type_u16: u16 = ((marker_or_prop >> 0) & 0xFFFF).try_into().unwrap();
        let prop_type = PropType::from_base_type(prop_type_u16);

        let prop_id_num: u16 = ((marker_or_prop >> 16) & 0xFFFF).try_into().unwrap();
        println!("prop ID num is {:#06X}", prop_id_num);
        let prop_id = if prop_id_num < 0x8000 {
            // tagged property ID
            PropertyId::Tagged { tag: prop_id_num }
        } else {
            // named property ID

            let mut property_set_guid_buf = [0u8; 16];
            file.read_exact(&mut property_set_guid_buf)
                .expect("failed to read property set GUID");
            let property_set_guid = Uuid::from_bytes_le(property_set_guid_buf);

            let identifier_type = file.read_u8()
                .expect("failed to read property identifier type");
            let property_name_info = match identifier_type {
                0x00 => {
                    // display ID
                    let disp_id = file.read_u32_le()
                        .expect("failed to read display ID");
                    PropertyNameInfo::DisplayId(disp_id)
                },
                0x01 => {
                    // name; NUL-terminated UTF-16 string
                    let mut words = Vec::new();
                    loop {
                        let word = file.read_u16_le()
                            .expect("failed to read property name word");
                        if word == 0x0000 {
                            break;
                        }
                        words.push(word);
                    }
                    let name = String::from_utf16(&words)
                        .expect("property name is invalid UTF-16");
                    PropertyNameInfo::Name(name)
                },
                other => {
                    panic!("unknown property identifier type {:#04X}", other);
                },
            };
            PropertyId::Named { property_set: property_set_guid, name_info: property_name_info }
        };
        println!("{:?} {:?}", prop_type, prop_id);

        let prop_value = match prop_type {
            PropType::Unspecified => PropValue::Unspecified,
            PropType::Null => PropValue::Null,
            PropType::Integer16 => {
                let value = file.read_i16_le()
                    .expect("failed to read Integer16 value");
                PropValue::Integer16(value)
            },
            PropType::Integer32 => {
                let value = file.read_i32_le()
                    .expect("failed to read Integer32 value");
                PropValue::Integer32(value)
            },
            PropType::Floating32 => {
                let value = file.read_f32_le()
                    .expect("failed to read Floating32 value");
                PropValue::Floating32(value)
            },
            PropType::Floating64 => {
                let value = file.read_f64_le()
                    .expect("failed to read Floating64 value");
                PropValue::Floating64(value)
            },
            PropType::Currency => {
                let value = file.read_i64_le()
                    .expect("failed to read Integer64 value");
                PropValue::Currency(value)
            },
            PropType::FloatingTime => {
                let value = file.read_f64_le()
                    .expect("failed to read FloatingTime value");
                PropValue::FloatingTime(value)
            },
            PropType::ErrorCode => {
                let value = file.read_u32_le()
                    .expect("failed to read ErrorCode value");
                PropValue::ErrorCode(value)
            },
            PropType::Boolean => {
                // boolean values are padded to 16 bits
                let value_word = file.read_u16_le()
                    .expect("failed to read Boolean value");
                let value = match value_word {
                    0x00 => false,
                    0x01 => true,
                    other => panic!("invalid Boolean value: {:#04X}", other),
                };
                PropValue::Boolean(value)
            },
            PropType::Integer64 => {
                let value = file.read_i64_le()
                    .expect("failed to read Integer64 value");
                PropValue::Integer64(value)
            },
            PropType::Time => {
                let value = file.read_i64_le()
                    .expect("failed to read Time value");
                PropValue::Time(value)
            },
            PropType::Guid => {
                let mut buf = [0u8; 16];
                file.read_exact(&mut buf)
                    .expect("failed to read Guid value");
                let guid = Uuid::from_bytes_le(buf);
                PropValue::Guid(guid)
            },
            PropType::Object => {
                let value_count = file.read_u32_le()
                    .expect("failed to read Object value count");
                if value_count != 1 {
                    panic!("Object value count {} instead of 1", value_count);
                }

                let byte_count_u32 = file.read_u32_le()
                    .expect("failed to read Object size");
                let byte_count: usize = byte_count_u32.try_into().unwrap();
                let mut bytes = vec![0u8; byte_count];
                file.read_exact(&mut bytes)
                    .expect("failed to read Object value");
                PropValue::Object(bytes)
            },
            PropType::Binary => {
                let byte_count_u32 = file.read_u32_le()
                    .expect("failed to read Binary size");
                let byte_count: usize = byte_count_u32.try_into().unwrap();
                let mut bytes = vec![0u8; byte_count];
                file.read_exact(&mut bytes)
                    .expect("failed to read Binary value");

                PropValue::Binary(bytes)
            }
            PropType::String8|PropType::MultipleString8 => {
                let value_count = file.read_u32_le()
                    .expect("failed to read (Multiple)String8 value count");
                if prop_type == PropType::String8 && value_count != 1 {
                    panic!("String8 value count {} instead of 1", value_count);
                }

                let mut values = Vec::with_capacity(value_count.try_into().unwrap());
                for _ in 0..value_count {
                    let byte_count_u32 = file.read_u32_le()
                        .expect("failed to read (Multiple)String8 size");
                    let byte_count: usize = byte_count_u32.try_into().unwrap();
                    let mut bytes = vec![0u8; byte_count];
                    file.read_exact(&mut bytes)
                        .expect("failed to read (Multiple)String8 value");
                    let string = String::from_utf8(bytes)
                        .expect("(Multiple)String8 value is not UTF-8");
                    values.push(string);
                }

                if prop_type == PropType::String8 {
                    PropValue::String8(values.swap_remove(0))
                } else {
                    PropValue::MultipleString8(values)
                }
            },
            PropType::String|PropType::MultipleString => {
                let value_count = file.read_u32_le()
                    .expect("failed to read (Multiple)String value count");
                if prop_type == PropType::String && value_count != 1 {
                    panic!("String value count {} instead of 1", value_count);
                }

                let mut values = Vec::with_capacity(value_count.try_into().unwrap());
                for _ in 0..value_count {
                    let byte_count_u32 = file.read_u32_le()
                        .expect("failed to read (Multiple)String size");
                    let byte_count: usize = byte_count_u32.try_into().unwrap();
                    if byte_count % 2 != 0 {
                        panic!("(Multiple)String value has odd size {}", byte_count);
                    }
                    let mut bytes = vec![0u8; byte_count];
                    file.read_exact(&mut bytes)
                        .expect("failed to read (Multiple)String value");
                    let mut words = Vec::with_capacity(bytes.len() / 2);
                    for chunk in bytes.chunks(2) {
                        let word = u16::from_le_bytes(chunk.try_into().unwrap());
                        words.push(word);
                    }
                    let string = String::from_utf16(&words)
                        .expect("(Multiple)String value is not UTF-16");
                    values.push(string);
                }

                if prop_type == PropType::String {
                    PropValue::String(values.swap_remove(0))
                } else {
                    PropValue::MultipleString(values)
                }
            },
            PropType::MultipleBinary => {
                let value_count = file.read_u32_le()
                    .expect("failed to read (Multiple)Binary value count");
                if prop_type == PropType::Binary && value_count != 1 {
                    panic!("Binary value count {} instead of 1", value_count);
                }

                let mut values = Vec::with_capacity(value_count.try_into().unwrap());
                for _ in 0..value_count {
                    let byte_count_u32 = file.read_u32_le()
                        .expect("failed to read (Multiple)Binary size");
                    let byte_count: usize = byte_count_u32.try_into().unwrap();
                    let mut bytes = vec![0u8; byte_count];
                    file.read_exact(&mut bytes)
                        .expect("failed to read (Multiple)Binary value");
                    values.push(bytes);
                }

                PropValue::MultipleBinary(values)
            },
            PropType::MultipleInteger16 => {
                let value_count = file.read_u32_le()
                    .expect("failed to read MultipleInteger16 value count");
                let mut values = Vec::with_capacity(value_count.try_into().unwrap());
                for _ in 0..value_count {
                    let value = file.read_i16_le()
                        .expect("failed to read MultipleInteger16 value");
                    values.push(value);
                }
                PropValue::MultipleInteger16(values)
            },
            PropType::MultipleInteger32 => {
                let value_count = file.read_u32_le()
                    .expect("failed to read MultipleInteger32 value count");
                let mut values = Vec::with_capacity(value_count.try_into().unwrap());
                for _ in 0..value_count {
                    let value = file.read_i32_le()
                        .expect("failed to read MultipleInteger32 value");
                    values.push(value);
                }
                PropValue::MultipleInteger32(values)
            },
            PropType::MultipleFloating32 => {
                let value_count = file.read_u32_le()
                    .expect("failed to read MultipleFloating32 value count");
                let mut values = Vec::with_capacity(value_count.try_into().unwrap());
                for _ in 0..value_count {
                    let value = file.read_f32_le()
                        .expect("failed to read MultipleFloating32 value");
                    values.push(value);
                }
                PropValue::MultipleFloating32(values)
            },
            PropType::MultipleFloating64 => {
                let value_count = file.read_u32_le()
                    .expect("failed to read MultipleFloating64 value count");
                let mut values = Vec::with_capacity(value_count.try_into().unwrap());
                for _ in 0..value_count {
                    let value = file.read_f64_le()
                        .expect("failed to read MultipleFloating64 value");
                    values.push(value);
                }
                PropValue::MultipleFloating64(values)
            },
            PropType::MultipleCurrency => {
                let value_count = file.read_u32_le()
                    .expect("failed to read MultipleCurrency value count");
                let mut values = Vec::with_capacity(value_count.try_into().unwrap());
                for _ in 0..value_count {
                    let value = file.read_i64_le()
                        .expect("failed to read MultipleCurrency value");
                    values.push(value);
                }
                PropValue::MultipleCurrency(values)
            },
            PropType::MultipleFloatingTime => {
                let value_count = file.read_u32_le()
                    .expect("failed to read MultipleFloatingTime value count");
                let mut values = Vec::with_capacity(value_count.try_into().unwrap());
                for _ in 0..value_count {
                    let value = file.read_f64_le()
                        .expect("failed to read MultipleFloatingTime value");
                    values.push(value);
                }
                PropValue::MultipleFloatingTime(values)
            },
            PropType::MultipleInteger64 => {
                let value_count = file.read_u32_le()
                    .expect("failed to read MultipleInteger64 value count");
                let mut values = Vec::with_capacity(value_count.try_into().unwrap());
                for _ in 0..value_count {
                    let value = file.read_i64_le()
                        .expect("failed to read MultipleInteger64 value");
                    values.push(value);
                }
                PropValue::MultipleInteger64(values)
            },
            PropType::MultipleTime => {
                let value_count = file.read_u32_le()
                    .expect("failed to read MultipleTime value count");
                let mut values = Vec::with_capacity(value_count.try_into().unwrap());
                for _ in 0..value_count {
                    let value = file.read_i64_le()
                        .expect("failed to read MultipleTime value");
                    values.push(value);
                }
                PropValue::MultipleTime(values)
            },
            PropType::MultipleGuid => {
                let value_count = file.read_u32_le()
                    .expect("failed to read MultipleGuid value count");
                let mut values = Vec::with_capacity(value_count.try_into().unwrap());
                for _ in 0..value_count {
                    let mut buf = [0u8; 16];
                    file.read_exact(&mut buf)
                        .expect("failed to read MultipleGuid value");
                    let value = Uuid::from_bytes_le(buf);
                    values.push(value);
                }
                PropValue::MultipleGuid(values)
            },
            PropType::Other(prop_type) => {
                if prop_type & 0x80_00 == 0 {
                    panic!("unknown property type {:#06X}", prop_type);
                }

                // single string in specific encoding
                let codepage_number = prop_type & 0x7F_FF;
                let codepage = codepage::to_encoding(codepage_number)
                    .expect("failed to obtain encoding for codepage");
                let mut decoder = codepage.new_decoder_with_bom_removal();

                let byte_count_u32 = file.read_u32_le()
                    .expect("failed to read encoded string size");
                let byte_count: usize = byte_count_u32.try_into().unwrap();
                let mut bytes = vec![0u8; byte_count];
                file.read_exact(&mut bytes)
                    .expect("failed to read (Multiple)String value");

                let mut string = String::with_capacity(bytes.len());
                let mut byte_pos = 0;
                loop {
                    let (res, bytes_read) = decoder.decode_to_string_without_replacement(
                        &bytes[byte_pos..],
                        &mut string,
                        true,
                    );
                    byte_pos += bytes_read;
                    match res {
                        DecoderResult::InputEmpty => {
                            // perfect
                            break;
                        },
                        DecoderResult::OutputFull => {
                            string.reserve(512);
                            continue;
                        },
                        DecoderResult::Malformed(_, _) => {
                            panic!("malformed string encountered");
                        },
                    }
                }

                PropValue::String(string)
            },
        };
        println!("{:?} {:?} {:#?}", prop_type, prop_id, prop_value);
    }
}


fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().collect();
    if args.len() != 2 {
        eprintln!("Usage: ftdump FILE");
        return ExitCode::FAILURE;
    }

    let mut file = File::open(&args[1])
        .expect("failed to open file");

    loop {
        let Some(chunk_type) = file.read_u32_le_or_eof()
            .expect("failed to read chunk typr") else { break };
        let chunk_length = file.read_u32_le()
            .expect("failed to read chunk length");
        let mut buf = vec![0u8; chunk_length.try_into().unwrap()];
        file.read_exact(&mut buf)
            .expect("failed to read chunk");
        if chunk_type == 0x00_00_00_02 {
            // message
            println!("message chunk, {} bytes", buf.len());

            let mut tnef_file = Cursor::new(&buf);
            parse_message(&mut tnef_file);
        } else {
            println!("chunk {:#010X}: {:?}", chunk_type, buf);
        }
    }


    ExitCode::SUCCESS
}
