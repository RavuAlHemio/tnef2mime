use std::{fmt, io};
use std::io::{BufRead, Cursor, Read, Seek};

use cfb::CompoundFile;
use log::error;

use crate::binread::BinaryReader;
use crate::guid::Guid;
use crate::tnef::{PropTag, PropType, PropValue, TnefReadError};


pub const CFB_SIGNATURE: u64 = 0xE1_1A_B1_A1_E0_11_CF_D0;
pub const CFB_SIGNATURE_4BYTES: u32 = (CFB_SIGNATURE & 0xFF_FF_FF_FF) as u32;


#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub struct Msg {
    pub properties: Vec<Property>,
    pub recipients: Vec<Recipient>,
    pub attachments: Vec<Attachment>,
}

#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub struct Property {
    pub property_type: PropType,
    pub tag: PropTag,
    pub flags: u32,
    pub value: PropValue,
}

#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub struct Recipient {
    pub properties: Vec<Property>,
}

#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub struct Attachment {
    pub properties: Vec<Property>,
}


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


fn read_properties<R: BufRead + Seek>(msg: &mut CompoundFile<R>, path_prefix: &str, header_length: usize) -> Result<(Vec<u8>, Vec<Property>), TnefReadError> {
    let mut properties = Vec::new();
    let prop_path = format!("{}/__properties_version1.0", path_prefix);
    let mut prop_stream = msg.open_stream(&prop_path)?;

    let mut header = vec![0u8; header_length];
    prop_stream.read_exact(&mut header)?;

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

                let value_path = format!("{}/__substg1.0_{:04X}{:04X}", path_prefix, tag_u16, type_u16);
                let mut value_stream = match msg.open_stream(&value_path) {
                    Ok(vs) => vs,
                    Err(_) => {
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

                let value_path = format!("{}/__substg1.0_{:04X}{:04X}", path_prefix, tag_u16, type_u16);
                let mut value_stream = match msg.open_stream(&value_path) {
                    Ok(vs) => vs,
                    Err(_) => {
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

                let lengths_path = format!("{}/__substg1.0_{:04X}{:04X}", path_prefix, tag_u16, type_u16);
                let mut lengths_stream = match msg.open_stream(&lengths_path) {
                    Ok(ls) => ls,
                    Err(_) => {
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

                let mut value_bufs = Vec::with_capacity(value_count);
                for value_index in 0..value_count {
                    let value_path = format!("{}/__substg1.0_{:04X}{:04X}-{:08X}", path_prefix, tag_u16, type_u16, value_index);
                    let mut value_stream = match msg.open_stream(&value_path) {
                        Ok(vs) => vs,
                        Err(_) => {
                            error!("failed to open property {:04X}{:04X} value {} stream; skipping", tag_u16, type_u16, value_index);
                            continue;
                        },
                    };
                    let mut value_buf = Vec::new();
                    value_stream.read_to_end(&mut value_buf)?;
                    value_bufs.push(value_buf);
                }

                match property_type {
                    PropType::MultipleBinary => PropValue::MultipleBinary(value_bufs),
                    PropType::MultipleString => {
                        let mut values = Vec::with_capacity(value_bufs.len());
                        for (value_index, value_buf) in value_bufs.into_iter().enumerate() {
                            if value_buf.len() % 2 != 0 {
                                error!("multiple UTF-16 string property {:04X}{:04X} value {} has odd byte count {}; skipping", tag_u16, type_u16, value_index, value_buf.len());
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
                                    error!("UTF-16 string property {:04X}{:04X} value {} contains invalid data; skipping", tag_u16, type_u16, value_index);
                                    continue;
                                },
                            };
                            values.push(value);
                        }
                        PropValue::MultipleString(values)
                    },
                    PropType::MultipleString8 => {
                        let mut values = Vec::with_capacity(value_bufs.len());
                        for (value_index, value_buf) in value_bufs.into_iter().enumerate() {
                            // FIXME: assumes UTF-8
                            let value = match String::from_utf8(value_buf) {
                                Ok(v) => v,
                                Err(_) => {
                                    error!("multiple 8-bit string property {:04X}{:04X} value {} contains invalid UTF-8 data; skipping", tag_u16, type_u16, value_index);
                                    continue;
                                },
                            };
                            values.push(value);
                        }
                        PropValue::MultipleString8(values)
                    },
                    _ => unreachable!(),
                }
            },
        };
        properties.push(Property {
            property_type,
            tag,
            flags,
            value,
        });
    }
    Ok((header, properties))
}


pub fn read_cfb_msg<R: BufRead + Seek>(reader: R) -> Result<Msg, TnefReadError> {
    let mut msg = CompoundFile::open(reader)?;

    let (header_bytes, properties) = read_properties(&mut msg, "", 32)?;

    // header:
    // 0..8 reserved
    // 8..12 next_recipient_id
    // 12..16 next_attachment_id
    let recipient_count = u32::from_le_bytes(header_bytes[16..20].try_into().unwrap());
    let attachment_count = u32::from_le_bytes(header_bytes[20..24].try_into().unwrap());
    // 24..32 reserved

    let mut recipients = Vec::with_capacity(recipient_count.try_into().unwrap());
    for recipient_index in 0..recipient_count {
        let recipient_path = format!("/__recip_version1.0_#{:08X}", recipient_index);
        let (_header_bytes, recipient_properties) = read_properties(&mut msg, &recipient_path, 8)?;
        recipients.push(Recipient {
            properties: recipient_properties,
        });
    }

    let mut attachments = Vec::with_capacity(attachment_count.try_into().unwrap());
    for attachment_index in 0..attachment_count {
        let attachment_path = format!("/__attach_version1.0_#{:08X}", attachment_index);
        let (_header_bytes, attachment_properties) = read_properties(&mut msg, &attachment_path, 8)?;
        attachments.push(Attachment {
            properties: attachment_properties,
        });
    }

    Ok(Msg {
        properties,
        recipients,
        attachments,
    })
}


#[derive(Debug)]
pub enum RtfDecodeError {
    Io(io::Error),
    HeaderTooShort { expected: usize, obtained: usize },
    UnsupportedCompression { compression_type: u32 },
}
impl fmt::Display for RtfDecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e)
                => write!(f, "I/O error: {}", e),
            Self::HeaderTooShort { expected, obtained }
                => write!(f, "header too short (expected {} bytes, obtained {})", expected, obtained),
            Self::UnsupportedCompression { compression_type }
                => write!(f, "unsupported compression 0x{:08X}", compression_type),
        }
    }
}
impl std::error::Error for RtfDecodeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::HeaderTooShort { .. } => None,
            Self::UnsupportedCompression { .. } => None,
        }
    }
}
impl From<io::Error> for RtfDecodeError {
    fn from(value: io::Error) -> Self { Self::Io(value) }
}


const DICTIONARY_CAPACITY: usize = 4096;
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct CompressedRtfDict {
    data: Box<[u8]>,
    write_pos: usize,
    read_pos: usize,
}
impl CompressedRtfDict {
    fn read_from_read_pos(&mut self) -> u8 {
        let b = self.data[self.read_pos];
        self.read_pos = (self.read_pos + 1) % DICTIONARY_CAPACITY;
        b
    }

    fn write_at_write_pos(&mut self, value: u8) {
        self.data[self.write_pos] = value;
        self.write_pos = (self.write_pos + 1) % DICTIONARY_CAPACITY;
    }

    pub fn literal_read(&mut self, value: u8) {
        self.write_at_write_pos(value);
    }

    pub fn is_decompression_complete(&self, offset: u16) -> bool {
        usize::from(offset) == self.write_pos
    }

    pub fn reference_read(&mut self, offset: u16, length: u16) -> Vec<u8> {
        let offset_usize = usize::from(offset);
        let actual_length = usize::from(length) + 2;

        let mut ret = Vec::with_capacity(actual_length);
        self.read_pos = offset_usize;
        for _ in 0..actual_length {
            let b = self.read_from_read_pos();
            ret.push(b);
            self.write_at_write_pos(b);
        }
        ret
    }

    pub fn new() -> Self {
        const INIT_DICTIONARY: [u8; 207] = *b"{\\rtf1\\ansi\\mac\\deff0\\deftab720{\\fonttbl;}{\\f0\\fnil \\froman \\fswiss \\fmodern \\fscript \\fdecor MS Sans SerifSymbolArialTimes New RomanCourier{\\colortbl\\red0\\green0\\blue0\r\n\\par \\pard\\plain\\f0\\fs20\\b\\i\\u\\tab\\tx";

        let data_vec = vec![0u8; DICTIONARY_CAPACITY];
        let mut data = data_vec.into_boxed_slice();
        data[0..INIT_DICTIONARY.len()].copy_from_slice(&INIT_DICTIONARY);

        let write_pos = INIT_DICTIONARY.len();
        let read_pos = 0;

        Self {
            data,
            write_pos,
            read_pos,
        }
    }
}



pub fn decode_compressed_rtf(compressed: &[u8]) -> Result<Vec<u8>, RtfDecodeError> {
    if compressed.len() < 16 {
        return Err(RtfDecodeError::HeaderTooShort { expected: 16, obtained: compressed.len() });
    }
    let compressed_size = u32::from_le_bytes(compressed[0..4].try_into().unwrap());
    let raw_size = u32::from_le_bytes(compressed[4..8].try_into().unwrap());
    let compression_type = u32::from_le_bytes(compressed[8..12].try_into().unwrap());
    let crc = u32::from_le_bytes(compressed[12..16].try_into().unwrap());

    if compression_type == 0x414C454D {
        // "MELA", uncompressed
        return Ok(compressed[16..].to_vec());
    }
    if compression_type != 0x75465A4C {
        // not "LZFu"
        return Err(RtfDecodeError::UnsupportedCompression { compression_type });
    }

    let mut cursor = Cursor::new(&compressed[16..]);
    let mut dict = CompressedRtfDict::new();
    let mut ret = Vec::with_capacity(raw_size.try_into().unwrap());
    while let Some(control) = cursor.read_u8_or_eof()? {
        print!("control bits: ");
        for bit_index in 0..8 {
            if control & (1 << bit_index) == 0 {
                print!("0");
            } else {
                print!("1");
            }
        }
        println!();

        for bit_index in 0..8 {
            if control & (1 << bit_index) == 0 {
                // literal
                println!("literal byte");
                let literal = cursor.read_u8()?;
                println!("  0x{:02X}", literal);
                ret.push(literal);
                dict.literal_read(literal);
            } else {
                // dictionary reference
                println!("dict reference");
                let dict_ref = cursor.read_u16_be()?; // yes, big endian
                println!("  ref=0x{:04X}", dict_ref);

                let length = dict_ref & 0b1111;
                let offset = (dict_ref >> 4) & 0b1111_1111_1111;
                println!("  offset={} len={}", offset, length);

                if dict.is_decompression_complete(offset) {
                    break;
                }

                let bytes = dict.reference_read(offset, length);
                println!("  obtained bytes {:?}", bytes);
                ret.extend_from_slice(&bytes);
            }
        }
    }
    Ok(ret)
}
