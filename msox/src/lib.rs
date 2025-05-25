mod prop_enums;
mod tnef_enums;


use from_to_repr::from_to_other;
use uuid::Uuid;

pub use crate::prop_enums::PropTag;
pub use crate::tnef_enums::{TnefAttributeId, TnefAttributeLevel};


/// The type of an Exchange property.
#[derive(Clone, Copy, Debug)]
#[from_to_other(base_type = u16, derive_compare = "as_int")]
pub enum PropType {
    Unspecified = 0x0000,
    Null = 0x0001,
    Integer16 = 0x0002,
    Integer32 = 0x0003,
    Floating32 = 0x0004,
    Floating64 = 0x0005,
    Currency = 0x0006,
    FloatingTime = 0x0007,
    ErrorCode = 0x000A,
    Boolean = 0x000B,
    Object = 0x000D,
    Integer64 = 0x0014,
    String8 = 0x001E,
    String = 0x001F,
    Time = 0x0040,
    Guid = 0x0048,
    Binary = 0x0102,
    MultipleInteger16 = 0x1002,
    MultipleInteger32 = 0x1003,
    MultipleFloating32 = 0x1004,
    MultipleFloating64 = 0x1005,
    MultipleCurrency = 0x1006,
    MultipleFloatingTime = 0x1007,
    MultipleInteger64 = 0x1014,
    MultipleString8 = 0x101E,
    MultipleString = 0x101F,
    MultipleTime = 0x1040,
    MultipleGuid = 0x1048,
    MultipleBinary = 0x1102,
    Other(u16),
}

/// The value of an Exchange property.
#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub enum PropValue {
    Unspecified,
    Null,
    Integer16(i16),
    Integer32(i32),
    Floating32(f32),
    Floating64(f64),
    Currency(i64),
    FloatingTime(f64),
    ErrorCode(u32),
    Boolean(bool),
    Object(Vec<u8>),
    Integer64(i64),
    String8(String),
    String(String),
    Time(i64),
    Guid(Uuid),
    Binary(Vec<u8>),
    MultipleInteger16(Vec<i16>),
    MultipleInteger32(Vec<i32>),
    MultipleFloating32(Vec<f32>),
    MultipleFloating64(Vec<f64>),
    MultipleCurrency(Vec<i64>),
    MultipleFloatingTime(Vec<f64>),
    MultipleInteger64(Vec<i64>),
    MultipleString8(Vec<String>),
    MultipleString(Vec<String>),
    MultipleTime(Vec<i64>),
    MultipleGuid(Vec<Uuid>),
    MultipleBinary(Vec<Vec<u8>>),
}
