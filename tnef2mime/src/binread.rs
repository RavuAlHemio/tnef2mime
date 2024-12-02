use std::io;

macro_rules! declare_read {
    ($func_name:ident, $or_eof_func_name:ident, $type:ty) => {
        fn $func_name(&mut self) -> Result<$type, io::Error>;
        fn $or_eof_func_name(&mut self) -> Result<Option<$type>, io::Error>;
    };
}
macro_rules! declare_read_le_be {
    ($le_func_name:ident, $le_or_eof_func_name:ident, $be_func_name:ident, $be_or_eof_func_name:ident, $type:ty) => {
        declare_read!($le_func_name, $le_or_eof_func_name, $type);
        declare_read!($be_func_name, $be_or_eof_func_name, $type);
    };
}
macro_rules! impl_read {
    ($func_name:ident, $or_eof_func_name:ident, $type:ty, $byte_count:expr, $from_bytes_func_name:ident) => {
        fn $func_name(&mut self) -> Result<$type, io::Error> {
            let mut buf = [0u8; $byte_count];
            self.read_exact(&mut buf)?;
            Ok(<$type>::$from_bytes_func_name(buf))
        }

        fn $or_eof_func_name(&mut self) -> Result<Option<$type>, io::Error> {
            let mut buf = [0u8; $byte_count];

            // the first read may fail
            let bytes_read = self.read(&mut buf[0..1])?;
            if bytes_read == 0 {
                return Ok(None);
            }

            // the rest must not
            if $byte_count > 1 {
                self.read_exact(&mut buf[1..$byte_count])?;
            }

            Ok(Some(<$type>::$from_bytes_func_name(buf)))
        }
    };
}
macro_rules! impl_read_le_be {
    ($le_func_name:ident, $le_or_eof_func_name:ident, $be_func_name:ident, $be_or_eof_func_name:ident, $type:ty, $byte_count:expr, $from_le_bytes_func_name:ident, $from_be_bytes_func_name:ident) => {
        impl_read!($le_func_name, $le_or_eof_func_name, $type, $byte_count, $from_le_bytes_func_name);
        impl_read!($be_func_name, $be_or_eof_func_name, $type, $byte_count, $from_be_bytes_func_name);
    };
}
macro_rules! impl_read_signed {
    ($signed_func:ident, $signed_func_or_eof:ident, $signed_type:ty, $unsigned_func:ident, $unsigned_func_or_eof:ident) => {
        fn $signed_func(&mut self) -> Result<$signed_type, io::Error> {
            let val = self.$unsigned_func()?;
            Ok(val as $signed_type)
        }

        fn $signed_func_or_eof(&mut self) -> Result<Option<$signed_type>, io::Error> {
            match self.$unsigned_func_or_eof() {
                Ok(Some(val)) => Ok(Some(val as $signed_type)),
                Ok(None) => Ok(None),
                Err(e) => Err(e),
            }
        }
    };
}
macro_rules! impl_read_signed_le_be {
    ($le_signed_func:ident, $le_signed_func_or_eof:ident, $be_signed_func:ident, $be_signed_func_or_eof:ident, $signed_type:ty, $le_unsigned_func:ident, $le_unsigned_func_or_eof:ident, $be_unsigned_func:ident, $be_unsigned_func_or_eof:ident) => {
        impl_read_signed!($le_signed_func, $le_signed_func_or_eof, $signed_type, $le_unsigned_func, $le_unsigned_func_or_eof);
        impl_read_signed!($be_signed_func, $be_signed_func_or_eof, $signed_type, $be_unsigned_func, $be_unsigned_func_or_eof);
    };
}


pub trait BinaryReader {
    declare_read!(read_u8, read_u8_or_eof, u8);
    declare_read_le_be!(read_u16_le, read_u16_le_or_eof, read_u16_be, read_u16_be_or_eof, u16);
    declare_read_le_be!(read_u32_le, read_u32_le_or_eof, read_u32_be, read_u32_be_or_eof, u32);
    declare_read_le_be!(read_u64_le, read_u64_le_or_eof, read_u64_be, read_u64_be_or_eof, u64);
    declare_read_le_be!(read_f32_le, read_f32_le_or_eof, read_f32_be, read_f32_be_or_eof, f32);
    declare_read_le_be!(read_f64_le, read_f64_le_or_eof, read_f64_be, read_f64_be_or_eof, f64);
    fn pad_to_4(&mut self, bytes_read: usize) -> Result<(), io::Error>;

    impl_read_signed!(read_i8, read_i8_or_eof, i8, read_u8, read_u8_or_eof);
    impl_read_signed_le_be!(read_i16_le, read_i16_le_or_eof, read_i16_be, read_i16_be_or_eof, i16, read_u16_le, read_u16_le_or_eof, read_u16_be, read_u16_be_or_eof);
    impl_read_signed_le_be!(read_i32_le, read_i32_le_or_eof, read_i32_be, read_i32_be_or_eof, i32, read_u32_le, read_u32_le_or_eof, read_u32_be, read_u32_be_or_eof);
    impl_read_signed_le_be!(read_i64_le, read_i64_le_or_eof, read_i64_be, read_i64_be_or_eof, i64, read_u64_le, read_u64_le_or_eof, read_u64_be, read_u64_be_or_eof);
}

impl<R: io::Read> BinaryReader for R {
    impl_read!(read_u8, read_u8_or_eof, u8, 1, from_be_bytes);
    impl_read_le_be!(read_u16_le, read_u16_le_or_eof, read_u16_be, read_u16_be_or_eof, u16, 2, from_le_bytes, from_be_bytes);
    impl_read_le_be!(read_u32_le, read_u32_le_or_eof, read_u32_be, read_u32_be_or_eof, u32, 4, from_le_bytes, from_be_bytes);
    impl_read_le_be!(read_u64_le, read_u64_le_or_eof, read_u64_be, read_u64_be_or_eof, u64, 8, from_le_bytes, from_be_bytes);
    impl_read_le_be!(read_f32_le, read_f32_le_or_eof, read_f32_be, read_f32_be_or_eof, f32, 4, from_le_bytes, from_be_bytes);
    impl_read_le_be!(read_f64_le, read_f64_le_or_eof, read_f64_be, read_f64_be_or_eof, f64, 8, from_le_bytes, from_be_bytes);

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
