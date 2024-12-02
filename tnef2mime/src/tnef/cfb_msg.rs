use std::io::{BufRead, Read, Seek};

use cfb::CompoundFile;
use log::error;

use crate::binread::BinaryReader;
use crate::guid::Guid;
use crate::tnef::{PropTag, PropValue, TnefFile, TnefReadError};

use super::PropType;


pub const CFB_SIGNATURE: u64 = 0xE1_1A_B1_A1_E0_11_CF_D0;
pub const CFB_SIGNATURE_4BYTES: u32 = (CFB_SIGNATURE & 0xFF_FF_FF_FF) as u32;


macro_rules! match_multiple_fixed_property_type {
    (
        $property_type:expr, $tag_u16:expr, $type_u16:expr, $value_buf:expr
        $(, $variant:ident, $inner_type:ty, $chunk_size:expr)*
        $(,)?
    ) => {
        match $property_type {
            $(
                PropType::$variant => {
                    if $value_buf.len() % $chunk_size != 0 {
                        error!("{:?} property {:04X}{:04X} has byte count {} not divisible by {}; skipping", $property_type, $tag_u16, $type_u16, $value_buf.len(), $chunk_size);
                        continue;
                    }
                    let mut values = Vec::with_capacity($value_buf.len() / $chunk_size);
                    for slice in $value_buf.chunks($chunk_size) {
                        let value = <$inner_type>::from_le_bytes(slice.try_into().unwrap());
                        values.push(value);
                    }
                    PropValue::$variant(values)
                },
            )*
            _ => unreachable!(),
        }
    };
}


pub fn read_cfb_msg_as_tnef<R: BufRead + Seek>(reader: R) -> Result<TnefFile, TnefReadError> {
    let mut msg = CompoundFile::open(reader)?;

    // read file properties
    let mut properties = Vec::new();
    let prop_stream = msg.open_stream("/__properties_version1.0")?;
    let _reserved0 = prop_stream.read_u64_le()?;
    let next_recipient_id = prop_stream.read_u32_le()?;
    let next_attachment_id = prop_stream.read_u32_le()?;
    let recipient_count = prop_stream.read_u32_le()?;
    let attachment_count = prop_stream.read_u32_le()?;
    let _reserved1 = prop_stream.read_u64_le()?;

    while let Some(type_u16) = prop_stream.read_u16_le_or_eof()? {
        let property_type = PropType::from_base_type(type_u16);

        let tag_u16 = prop_stream.read_u16_le()?;
        let tag = PropTag::from_base_type(tag_u16);

        let flags = prop_stream.read_u32_le()?;

        let value = match property_type {
            PropType::Unspecified|PropType::Null
                    |PropType::Other(_) => return Err(TnefReadError::InvalidPropertyType { property_type: type_u16 }),
            PropType::Integer16|PropType::Integer32
                    |PropType::Floating32|PropType::Floating64
                    |PropType::Boolean|PropType::Currency
                    |PropType::FloatingTime|PropType::Time
                    |PropType::Integer64|PropType::ErrorCode => {
                // stored inline
                let mut buf = [0u8; 8];
                prop_stream.read_exact(&mut buf)?;

                match property_type {
                    PropType::Integer16 => PropValue::Integer16(i16::from_le_bytes(buf[0..2].try_into().unwrap())),
                    PropType::Integer32 => PropValue::Integer32(i32::from_le_bytes(buf[0..4].try_into().unwrap())),
                    PropType::Floating32 => PropValue::Floating32(f32::from_le_bytes(buf[0..4].try_into().unwrap())),
                    PropType::Floating64 => PropValue::Floating64(f64::from_le_bytes(buf[0..8].try_into().unwrap())),
                    PropType::Boolean => PropValue::Boolean(buf[0] != 0x00),
                    PropType::Currency => PropValue::Currency(i64::from_le_bytes(buf[0..8].try_into().unwrap())),
                    PropType::FloatingTime => PropValue::FloatingTime(f64::from_le_bytes(buf[0..8].try_into().unwrap())),
                    PropType::Time => PropValue::Time(i64::from_le_bytes(buf[0..8].try_into().unwrap())),
                    PropType::Integer64 => PropValue::Integer64(i64::from_le_bytes(buf[0..8].try_into().unwrap())),
                    PropType::ErrorCode => PropValue::ErrorCode(u32::from_le_bytes(buf[0..4].try_into().unwrap())),
                    _ => unreachable!(),
                }
            },
            PropType::String|PropType::Binary
                    |PropType::String8|PropType::Guid
                    |PropType::Object => {
                // stored externally
                let _length = prop_stream.read_u32_le()?;
                let _reserved2 = prop_stream.read_u32_le()?;

                let value_path = format!("__substg1.0_{:04X}{:04X}", tag_u16, type_u16);
                let value_stream = match msg.open_stream(&value_path) {
                    Ok(vs) => vs,
                    Err(e) => {
                        error!("failed to open property {:04X}{:04X} value stream; skipping", tag_u16, type_u16);
                        continue;
                    },
                };
                let mut value_buf = Vec::new();
                value_stream.read_to_end(&mut value_buf)?;

                match property_type {
                    PropType::String => {
                        if value_buf.len() % 2 != 0 {
                            error!("UTF-16 string property {:04X}{:04X} has odd byte count {}; skipping", tag_u16, type_u16, value_buf.len());
                            continue;
                        }
                        let mut words = Vec::with_capacity(value_buf.len() / 2);
                        for slice in value_buf.chunks(2) {
                            let word = u16::from_le_bytes(slice.try_into().unwrap());
                            words.push(word);
                        }
                        let value = match String::from_utf16(&words) {
                            Ok(v) => v,
                            Err(_) => {
                                error!("UTF-16 string property {:04X}{:04X} contains invalid data; skipping", tag_u16, type_u16);
                                continue;
                            },
                        };
                        PropValue::String(value)
                    },
                    PropType::Binary => PropValue::Binary(value_buf),
                    PropType::String8 => {
                        // FIXME: assumes UTF-8
                        let value = match String::from_utf8(value_buf) {
                            Ok(v) => v,
                            Err(_) => {
                                error!("8-bit string property {:04X}{:04X} contains invalid UTF-8 data; skipping", tag_u16, type_u16);
                                continue;
                            },
                        };
                        PropValue::String8(value)
                    },
                    PropType::Guid => {
                        if value_buf.len() != 16 {
                            error!("GUID property {:04X}{:04X} has {} bytes (expected 16 bytes); skipping", tag_u16, type_u16, value_buf.len());
                            continue;
                        }
                        let guid = Guid::from_le_byte_slice(&value_buf).unwrap();
                        PropValue::Guid(guid)
                    },
                    PropType::Object => PropValue::Object(value_buf),
                    _ => unreachable!(),
                }
            },
            PropType::MultipleInteger16|PropType::MultipleInteger32
                    |PropType::MultipleFloating32|PropType::MultipleFloating64
                    |PropType::MultipleCurrency|PropType::MultipleFloatingTime
                    |PropType::MultipleTime|PropType::MultipleGuid
                    |PropType::MultipleInteger64 => {
                // stored externally in one stream
                let _length = prop_stream.read_u32_le()?;
                let _reserved2 = prop_stream.read_u32_le()?;

                let value_path = format!("__substg1.0_{:04X}{:04X}", tag_u16, type_u16);
                let value_stream = match msg.open_stream(&value_path) {
                    Ok(vs) => vs,
                    Err(e) => {
                        error!("failed to open property {:04X}{:04X} value stream; skipping", tag_u16, type_u16);
                        continue;
                    },
                };
                let mut value_buf = Vec::new();
                value_stream.read_to_end(&mut value_buf)?;

                match_multiple_fixed_property_type!(
                    property_type, tag_u16, type_u16, value_buf,
                    MultipleInteger16, i16, 2,
                    MultipleInteger32, i32, 4,
                    MultipleFloating32, f32, 4,
                    MultipleFloating64, f64, 8,
                    MultipleCurrency, i64, 8,
                    MultipleFloatingTime, f64, 8,
                    MultipleTime, i64, 8,
                    MultipleGuid, Guid, 16,
                    MultipleInteger64, i64, 8,
                )
            },
            PropType::MultipleBinary|PropType::MultipleString8
                    |PropType::MultipleString => {
                // stored externally in multiple streams
                let _length = prop_stream.read_u32_le()?;
                let _reserved2 = prop_stream.read_u32_le()?;

                let lengths_path = format!("/__substg1.0_{:04X}{:04X}", tag_u16, type_u16);
                let lengths_stream = match msg.open_stream(&lengths_path) {
                    Ok(ls) => ls,
                    Err(e) => {
                        error!("failed to open property {:04X}{:04X} length stream; skipping", tag_u16, type_u16);
                        continue;
                    },
                };
                let mut lengths_buf = Vec::new();
                lengths_stream.read_to_end(&mut lengths_buf)?;

                let value_count = match property_type {
                    PropType::MultipleString|PropType::MultipleString8 => {
                        // lengths are 4 bytes a piece
                        if lengths_buf.len() % 4 != 0 {
                            error!("{:?} property {:04X}{:04X} length stream has byte count {} not divisible by 4; skipping", property_type, tag_u16, type_u16, lengths_buf.len());
                            continue;
                        }
                        lengths_buf.len() / 4
                    },
                    PropType::MultipleBinary => {
                        // lengths are 8 bytes a piece but the latter 4 bytes are reserved
                        if lengths_buf.len() % 8 != 0 {
                            error!("{:?} property {:04X}{:04X} length stream has byte count {} not divisible by 8; skipping", property_type, tag_u16, type_u16, lengths_buf.len());
                            continue;
                        }
                        lengths_buf.len() / 8
                    },
                    _ => unreachable!(),
                };

                let value_bufs = Vec::with_capacity(value_count);
                for value_index in 0..value_count {
                    let value_path = format!("/__substg1.0_{:04X}{:04X}-{:08X}", tag_u16, type_u16, value_index);
                    let value_stream = match msg.open_stream(&value_path) {
                        Ok(vs) => vs,
                        Err(e) => {
                            error!("failed to open property {:04X}{:04X} value {} stream; skipping", tag_u16, type_u16, value_index);
                            continue;
                        },
                    };
                    let value_buf = Vec::new();
                    value_stream.read_to_end(&mut value_buf)?;
                    value_bufs.push(value_buf);
                }

                match property_type {
                    PropType::MultipleString => {
                        for (value_index, value_buf) in value_bufs.into_iter().enumerate() {
                            if lengths_buf.len() % 2 != 0 {
                                error!("multiple UTF-16 string property {:04X}{:04X} value {} has odd byte count {}; skipping", tag_u16, type_u16, value_index, lengths_buf.len());
                                continue;
                            }
                            let mut words = Vec::with_capacity(lengths_buf.len() / 2);
                            for slice in lengths_buf.chunks(2) {
                                let word = u16::from_le_bytes(slice.try_into().unwrap());
                                words.push(word);
                            }
                            let value = match String::from_utf16(&words) {
                                Ok(v) => v,
                                Err(_) => {
                                    error!("UTF-16 string property {:04X}{:04X} contains invalid data; skipping", tag_u16, type_u16);
                                    continue;
                                },
                            };
                            value
                        }
                        PropValue::String(value)
                    },
                    PropType::Binary => PropValue::Binary(lengths_buf),
                    PropType::String8 => {
                        // FIXME: assumes UTF-8
                        let value = match String::from_utf8(lengths_buf) {
                            Ok(v) => v,
                            Err(_) => {
                                error!("8-bit string property {:04X}{:04X} contains invalid UTF-8 data; skipping", tag_u16, type_u16);
                                continue;
                            },
                        };
                        PropValue::String8(value)
                    },
                    PropType::Guid => {
                        if lengths_buf.len() != 16 {
                            error!("GUID property {:04X}{:04X} has {} bytes (expected 16 bytes); skipping", tag_u16, type_u16, lengths_buf.len());
                            continue;
                        }
                        let guid = Guid::from_le_byte_slice(&lengths_buf).unwrap();
                        PropValue::Guid(guid)
                    },
                    PropType::Object => PropValue::Object(lengths_buf),
                    _ => unreachable!(),
                }
            },
        };
    }

    todo!();
}
