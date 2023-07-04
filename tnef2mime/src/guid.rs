use std::fmt;


#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Guid {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}
impl Guid {
    pub fn from_le_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != 16 {
            return None;
        }

        let data1 =
            ((bytes[0] as u32) << 0)
            | ((bytes[1] as u32) << 8)
            | ((bytes[2] as u32) << 16)
            | ((bytes[3] as u32) << 24)
        ;
        let data2 =
            ((bytes[4] as u16) << 0)
            | ((bytes[5] as u16) << 8)
        ;
        let data3 =
            ((bytes[6] as u16) << 0)
            | ((bytes[7] as u16) << 8)
        ;
        let data4 = [
            bytes[8], bytes[9], bytes[10], bytes[11],
            bytes[12], bytes[13], bytes[14], bytes[15],
        ];

        Some(Self {
            data1,
            data2,
            data3,
            data4,
        })
    }

    pub fn from_be_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != 16 {
            return None;
        }

        let data1 =
            ((bytes[0] as u32) << 24)
            | ((bytes[1] as u32) << 16)
            | ((bytes[2] as u32) << 8)
            | ((bytes[3] as u32) << 0)
        ;
        let data2 =
            ((bytes[4] as u16) << 8)
            | ((bytes[5] as u16) << 0)
        ;
        let data3 =
            ((bytes[6] as u16) << 8)
            | ((bytes[7] as u16) << 0)
        ;
        let data4 = [
            bytes[8], bytes[9], bytes[10], bytes[11],
            bytes[12], bytes[13], bytes[14], bytes[15],
        ];

        Some(Self {
            data1,
            data2,
            data3,
            data4,
        })
    }
}
impl fmt::Display for Guid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f,
            "{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
            self.data1, self.data2, self.data3,
            self.data4[0], self.data4[1], self.data4[2], self.data4[3], self.data4[4], self.data4[5], self.data4[6], self.data4[7],
        )
    }
}
