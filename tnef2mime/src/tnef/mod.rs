pub(crate) mod cfb_msg;


use std::fmt;
use std::io::{self, BufRead};
use std::string::FromUtf16Error;

use encoding_rs::Encoding;
use from_to_repr::FromToRepr;
use log::{debug, error, warn};
use msox::{PropTag, PropType, PropValue, TnefAttributeId, TnefAttributeLevel};
use uuid::Uuid;

use crate::binread::BinaryReader;


pub const TNEF_SIGNATURE: u32 = 0x223E9F78;


#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TnefFile {
    pub legacy_key: u16,
    pub attributes: Vec<TnefAttribute>,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TnefAttribute {
    pub level: TnefAttributeLevel,
    pub id: TnefAttributeId,
    pub data: Vec<u8>,
    pub checksum: u16,
}

#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub struct Property {
    pub tag: PropTag,
    pub id: Option<(Uuid, PropId)>,
    pub value: PropValue,
}

#[derive(Clone, Debug, Eq, FromToRepr, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u32)]
pub enum PropIdType {
    Number = 0x00_00_00_00,
    String = 0x00_00_00_01,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PropId {
    Number(u32),
    String(String),
}


#[derive(Debug)]
pub enum TnefReadError {
    Io(std::io::Error),
    Signature { expected: u32, obtained: u32 },
    LengthConversion { obtained: i32 },
    ChecksumMismatch { obtained: u16, calculated: u16 },
    InvalidIdType { obtained: u32 },
    InvalidStringId { obtained: Vec<u16>, error: FromUtf16Error },
    InvalidBoolean { obtained: u8 },
    MultipleValuesSingleType { prop_type: PropType, count: u32 },
    InvalidString { obtained: Vec<u16>, error: FromUtf16Error },
    OddStringLength { byte_length: usize },
    InvalidPropertyType { property_type: u16 },
}
impl fmt::Display for TnefReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O error: {}", e),
            Self::Signature { expected, obtained }
                => write!(f, "wrong TNEF signature (expected 0x{:08X}, obtained 0x{:08X})", expected, obtained),
            Self::LengthConversion { obtained }
                => write!(f, "failed to convert length ({}) from i32 to usize", obtained),
            Self::ChecksumMismatch { obtained, calculated }
                => write!(f, "checksum mismatch: calculated 0x{:04X}, obtained 0x{:04X}", calculated, obtained),
            Self::InvalidIdType { obtained }
                => write!(f, "invalid ID type (obtained 0x{:08X})", obtained),
            Self::InvalidStringId { obtained, error }
                => write!(f, "invalid string ID: {} (obtained {:?})", error, obtained),
            Self::InvalidBoolean { obtained }
                => write!(f, "invalid boolean value 0x{:02X} (must be 0x00 for false or 0x01 for true)", obtained),
            Self::MultipleValuesSingleType { prop_type, count }
                => write!(f, "more than one value ({}) specified with type {:?}", count, prop_type),
            Self::InvalidString { obtained, error }
                => write!(f, "invalid UTF-16 string: {} (obtained {:?})", error, obtained),
            Self::OddStringLength { byte_length }
                => write!(f, "odd length {} of UTF-16 string", byte_length),
            Self::InvalidPropertyType { property_type }
                => write!(f, "invalid property type 0x{:04X}", property_type),
        }
    }
}
impl std::error::Error for TnefReadError {
}
impl From<std::io::Error> for TnefReadError {
    fn from(e: std::io::Error) -> Self { Self::Io(e) }
}


pub fn read_tnef<R: BufRead>(mut reader: R) -> Result<TnefFile, TnefReadError> {
    // read signature
    let signature = reader.read_u32_le()?;
    if signature != TNEF_SIGNATURE {
        return Err(TnefReadError::Signature { expected: TNEF_SIGNATURE, obtained: signature });
    }

    // obtain legacy key
    let legacy_key = reader.read_u16_le()?;

    let mut attributes = Vec::new();
    loop {
        // anything left?
        let attrib_level_u8 = match reader.read_u8() {
            Ok(al) => al,
            Err(e) => {
                if e.kind() == io::ErrorKind::UnexpectedEof {
                    break;
                } else {
                    return Err(e.into());
                }
            },
        };
        let attrib_level: TnefAttributeLevel = attrib_level_u8.into();

        let attrib_id_u32 = reader.read_u32_le()?;
        let attrib_id: TnefAttributeId = attrib_id_u32.into();

        let length_i32 = reader.read_i32_le()?;
        let length: usize = match length_i32.try_into() {
            Ok(val) => val,
            Err(_) => return Err(TnefReadError::LengthConversion { obtained: length_i32 }),
        };

        let mut data_buf = vec![0u8; length];
        reader.read_exact(&mut data_buf)?;

        let checksum = reader.read_u16_le()?;

        // calculate checksum
        let mut my_checksum = 0u16;
        for &b in &data_buf {
            my_checksum = my_checksum.wrapping_add(b.into());
        }

        if checksum != my_checksum {
            return Err(TnefReadError::ChecksumMismatch { obtained: checksum, calculated: my_checksum });
        }

        attributes.push(TnefAttribute {
            level: attrib_level,
            id: attrib_id,
            data: data_buf,
            checksum,
        })
    }

    Ok(TnefFile {
        legacy_key,
        attributes,
    })
}

fn decode_property<R: BufRead>(mut reader: R, encoding: &'static Encoding) -> Result<Property, TnefReadError> {
    debug!("new property");

    let prop_type_u16 = reader.read_u16_le()?;
    debug!("prop type: {0} (0x{0:04x})", prop_type_u16);
    let prop_type: PropType = prop_type_u16.into();
    debug!("prop type: {:?}", prop_type);

    let prop_tag_u16 = reader.read_u16_le()?;
    debug!("prop tag: {0} (0x{0:04x})", prop_tag_u16);
    let prop_tag: PropTag = prop_tag_u16.into();
    debug!("prop tag: {:?}", prop_tag);

    let prop_full_id = if prop_tag_u16 >= 0x8000 {
        // named property
        let mut guid_buf = [0u8; 16];
        reader.read_exact(&mut guid_buf)?;
        let guid = Uuid::from_slice_le(&guid_buf).unwrap();
        debug!("guid: {}", guid);

        let id_type_u32 = reader.read_u32_le()?;
        debug!("id type: {0} (0x{0:08x})", id_type_u32);
        let id_type: PropIdType = match id_type_u32.try_into() {
            Ok(it) => it,
            Err(value) => return Err(TnefReadError::InvalidIdType { obtained: value }),
        };
        debug!("id type: {:?}", id_type);

        let id = match id_type {
            PropIdType::Number => {
                let prop_id = reader.read_u32_le()?;
                reader.pad_to_4(4)?;
                debug!("numeric prop id: {0} (0x{0:08x})", prop_id);
                PropId::Number(prop_id)
            },
            PropIdType::String => {
                let length_bytes = reader.read_u32_le()?;
                debug!("prop name length: {0} (0x{0:08x})", length_bytes);
                if length_bytes % 2 != 0 {
                    warn!("prop name length not divisible by 2?!");
                }
                let length_chars: usize = usize::try_from(length_bytes).unwrap() / 2;
                let mut chars = Vec::with_capacity(length_chars);
                for _ in 0..length_chars {
                    let char = reader.read_u16_le()?;
                    chars.push(char);
                }

                // swallow padding
                reader.pad_to_4(length_bytes.try_into().unwrap())?;

                let prop_id = match String::from_utf16(&chars) {
                    Ok(pi) => pi,
                    Err(e) => return Err(TnefReadError::InvalidStringId { obtained: chars, error: e }),
                };
                debug!("prop name: {}", prop_id);
                PropId::String(prop_id)
            },
        };

        Some((guid, id))
    } else {
        None
    };

    let prop_value = match prop_type {
        PropType::Unspecified => PropValue::Unspecified,
        PropType::Null => PropValue::Null,
        PropType::Integer16 => {
            let val = reader.read_i16_le()?;
            reader.pad_to_4(2)?;
            PropValue::Integer16(val)
        },
        PropType::Integer32 => {
            let val = reader.read_i32_le()?;
            reader.pad_to_4(4)?;
            PropValue::Integer32(val)
        },
        PropType::Floating32 => {
            let val = reader.read_f32_le()?;
            reader.pad_to_4(4)?;
            PropValue::Floating32(val)
        },
        PropType::Floating64 => {
            let val = reader.read_f64_le()?;
            reader.pad_to_4(8)?;
            PropValue::Floating64(val)
        },
        PropType::Currency => {
            let val = reader.read_i64_le()?;
            reader.pad_to_4(8)?;
            PropValue::Currency(val)
        },
        PropType::FloatingTime => {
            let val = reader.read_f64_le()?;
            reader.pad_to_4(8)?;
            PropValue::FloatingTime(val)
        },
        PropType::ErrorCode => {
            let val = reader.read_u32_le()?;
            reader.pad_to_4(4)?;
            PropValue::ErrorCode(val)
        },
        PropType::Boolean => {
            let b = reader.read_u8()?;
            let val = match b {
                0x00 => false,
                0x01 => true,
                other => return Err(TnefReadError::InvalidBoolean { obtained: other }),
            };
            reader.pad_to_4(1)?;
            PropValue::Boolean(val)
        },
        PropType::Object => {
            let value_count = reader.read_u32_le()?;
            if value_count != 1 {
                return Err(TnefReadError::MultipleValuesSingleType { prop_type, count: value_count });
            }

            let byte_count_u32 = reader.read_u32_le()?;
            let byte_count: usize = byte_count_u32.try_into().unwrap();
            let mut bytes = vec![0u8; byte_count];
            reader.read_exact(&mut bytes)?;

            // possible padding
            reader.pad_to_4(byte_count)?;

            PropValue::Object(bytes)
        },
        PropType::Integer64 => {
            let val = reader.read_i64_le()?;
            reader.pad_to_4(8)?;
            PropValue::Integer64(val)
        },
        PropType::Time => {
            let val = reader.read_i64_le()?;
            reader.pad_to_4(8)?;
            PropValue::Time(val)
        },
        PropType::Guid => {
            let mut buf = [0u8; 16];
            reader.read_exact(&mut buf)?;
            let guid = Uuid::from_slice_le(&buf).unwrap();
            PropValue::Guid(guid)
        },
        PropType::MultipleInteger16 => {
            let value_count = reader.read_u32_le()?;
            let mut vals = Vec::with_capacity(value_count.try_into().unwrap());
            for _ in 0..value_count {
                let val = reader.read_i16_le()?;
                reader.pad_to_4(2)?;
                vals.push(val);
            }
            PropValue::MultipleInteger16(vals)
        },
        PropType::MultipleInteger32 => {
            let value_count = reader.read_u32_le()?;
            let mut vals = Vec::with_capacity(value_count.try_into().unwrap());
            for _ in 0..value_count {
                let val = reader.read_i32_le()?;
                reader.pad_to_4(4)?;
                vals.push(val);
            }
            PropValue::MultipleInteger32(vals)
        },
        PropType::MultipleFloating32 => {
            let value_count = reader.read_u32_le()?;
            let mut vals = Vec::with_capacity(value_count.try_into().unwrap());
            for _ in 0..value_count {
                let val = reader.read_f32_le()?;
                reader.pad_to_4(4)?;
                vals.push(val);
            }
            PropValue::MultipleFloating32(vals)
        },
        PropType::MultipleFloating64 => {
            let value_count = reader.read_u32_le()?;
            let mut vals = Vec::with_capacity(value_count.try_into().unwrap());
            for _ in 0..value_count {
                let val = reader.read_f64_le()?;
                reader.pad_to_4(8)?;
                vals.push(val);
            }
            PropValue::MultipleFloating64(vals)
        },
        PropType::MultipleCurrency => {
            let value_count = reader.read_u32_le()?;
            let mut vals = Vec::with_capacity(value_count.try_into().unwrap());
            for _ in 0..value_count {
                let val = reader.read_i64_le()?;
                reader.pad_to_4(8)?;
                vals.push(val);
            }
            PropValue::MultipleCurrency(vals)
        },
        PropType::MultipleFloatingTime => {
            let value_count = reader.read_u32_le()?;
            let mut vals = Vec::with_capacity(value_count.try_into().unwrap());
            for _ in 0..value_count {
                let val = reader.read_f64_le()?;
                reader.pad_to_4(8)?;
                vals.push(val);
            }
            PropValue::MultipleFloatingTime(vals)
        },
        PropType::MultipleInteger64 => {
            let value_count = reader.read_u32_le()?;
            let mut vals = Vec::with_capacity(value_count.try_into().unwrap());
            for _ in 0..value_count {
                let val = reader.read_i64_le()?;
                reader.pad_to_4(4)?;
                vals.push(val);
            }
            PropValue::MultipleInteger64(vals)
        },
        PropType::String8|PropType::MultipleString8 => {
            let value_count = reader.read_u32_le()?;
            if prop_type == PropType::String8 && value_count != 1 {
                return Err(TnefReadError::MultipleValuesSingleType { prop_type, count: value_count });
            }
            let mut values = Vec::with_capacity(value_count.try_into().unwrap());

            for _ in 0..value_count {
                let byte_count_u32 = reader.read_u32_le()?;
                let byte_count: usize = byte_count_u32.try_into().unwrap();
                let mut bytes = vec![0u8; byte_count];
                reader.read_exact(&mut bytes)?;

                let (cow_string, _bad_sequences) = encoding.decode_with_bom_removal(&bytes);
                let string = cow_string.into_owned();

                // possible padding
                reader.pad_to_4(byte_count)?;

                values.push(string);
            }

            if prop_type == PropType::String8 {
                PropValue::String8(values.remove(0))
            } else {
                assert_eq!(prop_type, PropType::MultipleString8);
                PropValue::MultipleString8(values)
            }
        },
        PropType::String|PropType::MultipleString => {
            let value_count = reader.read_u32_le()?;
            debug!("string has {} values", value_count);
            if prop_type == PropType::String && value_count != 1 {
                return Err(TnefReadError::MultipleValuesSingleType { prop_type, count: value_count });
            }
            let mut values = Vec::with_capacity(value_count.try_into().unwrap());

            for _ in 0..value_count {
                let byte_count_u32 = reader.read_u32_le()?;
                let byte_count: usize = byte_count_u32.try_into().unwrap();
                debug!("string value has {} bytes", byte_count);
                if byte_count % 2 != 0 {
                    return Err(TnefReadError::OddStringLength { byte_length: byte_count });
                }
                let char_count = byte_count / 2;
                let mut chars = Vec::with_capacity(char_count);
                for _ in 0..char_count {
                    let char = reader.read_u16_le()?;
                    chars.push(char);
                }

                let string = match String::from_utf16(&chars) {
                    Ok(s) => s,
                    Err(e) => return Err(TnefReadError::InvalidString { error: e, obtained: chars }),
                };

                // possible padding
                reader.pad_to_4(char_count * 2)?;

                values.push(string);
            }

            if prop_type == PropType::String {
                PropValue::String(values.remove(0))
            } else {
                assert_eq!(prop_type, PropType::MultipleString);
                PropValue::MultipleString(values)
            }
        },
        PropType::MultipleTime => {
            let value_count = reader.read_u32_le()?;
            let mut vals = Vec::with_capacity(value_count.try_into().unwrap());
            for _ in 0..value_count {
                let val = reader.read_i64_le()?;
                reader.pad_to_4(4)?;
                vals.push(val);
            }
            PropValue::MultipleTime(vals)
        },
        PropType::MultipleGuid => {
            let value_count = reader.read_u32_le()?;
            let mut vals = Vec::with_capacity(value_count.try_into().unwrap());
            for _ in 0..value_count {
                let mut buf = [0u8; 16];
                reader.read_exact(&mut buf)?;
                let guid = Uuid::from_slice_le(&buf).unwrap();
                vals.push(guid)
            }
            PropValue::MultipleGuid(vals)
        },
        PropType::Binary|PropType::MultipleBinary => {
            let value_count = reader.read_u32_le()?;
            debug!("binary value count: {}", value_count);
            if prop_type == PropType::Binary && value_count != 1 {
                return Err(TnefReadError::MultipleValuesSingleType { prop_type, count: value_count });
            }
            let mut values = Vec::with_capacity(value_count.try_into().unwrap());

            for _ in 0..value_count {
                let byte_count_u32 = reader.read_u32_le()?;
                let byte_count: usize = byte_count_u32.try_into().unwrap();
                debug!("byte count: {}", byte_count);
                let mut bytes = vec![0u8; byte_count];
                reader.read_exact(&mut bytes)?;

                // possible padding
                reader.pad_to_4(byte_count)?;

                values.push(bytes);
            }

            if prop_type == PropType::Binary {
                PropValue::Binary(values.remove(0))
            } else {
                assert_eq!(prop_type, PropType::MultipleBinary);
                PropValue::MultipleBinary(values)
            }
        },
        PropType::Other(other) => {
            let mut buf = [0u8; 128];
            reader.read_exact(&mut buf)?;
            error!("unknown type {}", other);
            crate::hexdump(&buf, "");
            panic!();
        },
    };

    let prop = Property {
        tag: prop_tag,
        id: prop_full_id,
        value: prop_value,
    };
    Ok(prop)
}

pub fn decode_properties<R: BufRead>(mut reader: R, encoding: &'static Encoding) -> Result<Vec<Property>, TnefReadError> {
    let prop_count: usize = reader.read_u32_le()?.try_into().unwrap();
    debug!("prop count: {}", prop_count);
    let mut properties = Vec::with_capacity(prop_count);
    for _ in 0..prop_count {
        let property = decode_property(&mut reader, encoding)?;
        properties.push(property);
    }
    Ok(properties)
}

pub fn decode_property_lists<R: BufRead>(mut reader: R, encoding: &'static Encoding) -> Result<Vec<Vec<Property>>, TnefReadError> {
    let list_count: usize = reader.read_u32_le()?.try_into().unwrap();
    let mut property_lists = Vec::with_capacity(list_count);
    for _ in 0..list_count {
        let property_list = decode_properties(&mut reader, encoding)?;
        property_lists.push(property_list);
    }
    Ok(property_lists)
}
