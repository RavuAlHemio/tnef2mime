use std::io;


pub trait BinaryReader {
    fn read_u8(&mut self) -> Result<u8, io::Error>;
    fn read_u16_be(&mut self) -> Result<u16, io::Error>;
    fn read_u16_le(&mut self) -> Result<u16, io::Error>;
    fn read_u32_be(&mut self) -> Result<u32, io::Error>;
    fn read_u32_le(&mut self) -> Result<u32, io::Error>;
    fn read_u64_be(&mut self) -> Result<u64, io::Error>;
    fn read_u64_le(&mut self) -> Result<u64, io::Error>;
    fn read_f32_be(&mut self) -> Result<f32, io::Error>;
    fn read_f32_le(&mut self) -> Result<f32, io::Error>;
    fn read_f64_be(&mut self) -> Result<f64, io::Error>;
    fn read_f64_le(&mut self) -> Result<f64, io::Error>;
    fn pad_to_4(&mut self, bytes_read: usize) -> Result<(), io::Error>;

    fn read_i8(&mut self) -> Result<i8, io::Error> {
        let val = self.read_u8()?;
        Ok(val as i8)
    }
    fn read_i16_be(&mut self) -> Result<i16, io::Error> {
        let val = self.read_u16_be()?;
        Ok(val as i16)
    }
    fn read_i16_le(&mut self) -> Result<i16, io::Error> {
        let val = self.read_u16_le()?;
        Ok(val as i16)
    }
    fn read_i32_be(&mut self) -> Result<i32, io::Error> {
        let val = self.read_u32_be()?;
        Ok(val as i32)
    }
    fn read_i32_le(&mut self) -> Result<i32, io::Error> {
        let val = self.read_u32_le()?;
        Ok(val as i32)
    }
    fn read_i64_be(&mut self) -> Result<i64, io::Error> {
        let val = self.read_u64_be()?;
        Ok(val as i64)
    }
    fn read_i64_le(&mut self) -> Result<i64, io::Error> {
        let val = self.read_u64_le()?;
        Ok(val as i64)
    }
}

impl<R: io::Read> BinaryReader for R {
    fn read_u8(&mut self) -> Result<u8, io::Error> {
        let mut buf = [0u8];
        self.read_exact(&mut buf)?;
        Ok(buf[0])
    }

    fn read_u16_be(&mut self) -> Result<u16, io::Error> {
        let mut buf = [0u8; 2];
        self.read_exact(&mut buf)?;
        Ok(
            ((buf[0] as u16) << 8)
            | ((buf[1] as u16) << 0)
        )
    }

    fn read_u16_le(&mut self) -> Result<u16, io::Error> {
        let mut buf = [0u8; 2];
        self.read_exact(&mut buf)?;
        Ok(
            ((buf[0] as u16) << 0)
            | ((buf[1] as u16) << 8)
        )
    }

    fn read_u32_be(&mut self) -> Result<u32, io::Error> {
        let mut buf = [0u8; 4];
        self.read_exact(&mut buf)?;
        Ok(
            ((buf[0] as u32) << 24)
            | ((buf[1] as u32) << 16)
            | ((buf[2] as u32) << 8)
            | ((buf[3] as u32) << 0)
        )
    }

    fn read_u32_le(&mut self) -> Result<u32, io::Error> {
        let mut buf = [0u8; 4];
        self.read_exact(&mut buf)?;
        Ok(
            ((buf[0] as u32) << 0)
            | ((buf[1] as u32) << 8)
            | ((buf[2] as u32) << 16)
            | ((buf[3] as u32) << 24)
        )
    }

    fn read_u64_be(&mut self) -> Result<u64, io::Error> {
        let mut buf = [0u8; 8];
        self.read_exact(&mut buf)?;
        Ok(
            ((buf[0] as u64) << 56)
            | ((buf[1] as u64) << 48)
            | ((buf[2] as u64) << 40)
            | ((buf[3] as u64) << 32)
            | ((buf[4] as u64) << 24)
            | ((buf[5] as u64) << 16)
            | ((buf[6] as u64) << 8)
            | ((buf[7] as u64) << 0)
        )
    }

    fn read_u64_le(&mut self) -> Result<u64, io::Error> {
        let mut buf = [0u8; 8];
        self.read_exact(&mut buf)?;
        Ok(
            ((buf[0] as u64) << 0)
            | ((buf[1] as u64) << 8)
            | ((buf[2] as u64) << 16)
            | ((buf[3] as u64) << 24)
            | ((buf[4] as u64) << 32)
            | ((buf[5] as u64) << 40)
            | ((buf[6] as u64) << 48)
            | ((buf[7] as u64) << 56)
        )
    }

    fn read_f32_be(&mut self) -> Result<f32, io::Error> {
        let mut buf = [0u8; 4];
        self.read_exact(&mut buf)?;
        Ok(f32::from_be_bytes(buf))
    }

    fn read_f32_le(&mut self) -> Result<f32, io::Error> {
        let mut buf = [0u8; 4];
        self.read_exact(&mut buf)?;
        Ok(f32::from_le_bytes(buf))

    }

    fn read_f64_be(&mut self) -> Result<f64, io::Error> {
        let mut buf = [0u8; 8];
        self.read_exact(&mut buf)?;
        Ok(f64::from_be_bytes(buf))
    }

    fn read_f64_le(&mut self) -> Result<f64, io::Error> {
        let mut buf = [0u8; 8];
        self.read_exact(&mut buf)?;
        Ok(f64::from_le_bytes(buf))
    }

    #[inline]
    fn pad_to_4(&mut self, bytes_read: usize) -> Result<(), io::Error> {
        if bytes_read % 4 == 0 {
            return Ok(())
        }
        let mut pad_buf = [0u8; 3];
        let pad_count = 4 - (bytes_read % 4);
        self.read_exact(&mut pad_buf[0..pad_count])
    }
}
