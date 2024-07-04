use crate::errors::CogErr;
use byteorder::{BigEndian, ByteOrder, LittleEndian, ReadBytesExt};
use enum_primitive::FromPrimitive;
use http_range_client::BufferedHttpRangeClient;
use serde::{Deserialize, Serialize};
use std::io::{Cursor, Error, ErrorKind};

pub struct Cog {
    pub header: CogHeader,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CogHeader {
    byteorder: TIFFByteOrder,
    ifd_offset: u32,
}

impl CogHeader {
    fn parse<T: ByteOrder>(
        reader: &mut Cursor<&[u8]>,
        byteorder: TIFFByteOrder,
    ) -> Result<Self, CogErr> {
        let magic = match byteorder {
            TIFFByteOrder::LittleEndian => reader.read_u16::<LittleEndian>()?,
            TIFFByteOrder::BigEndian => reader.read_u16::<BigEndian>()?,
        };

        if magic != 42 {
            return Err(CogErr::IoError(std::io::Error::new(
                ErrorKind::InvalidData,
                "Invalid TIFF magic number",
            )));
        }

        let ifd_offset = reader.read_u32::<T>()?;

        Ok(Self {
            byteorder,
            ifd_offset,
        })
    }
}

impl Cog {
    pub async fn new(client: BufferedHttpRangeClient) -> Result<Self, CogErr> {
        let header = Self::fetch_header(client).await?;
        Ok(Self { header })
    }

    pub async fn fetch_header(mut client: BufferedHttpRangeClient) -> Result<CogHeader, CogErr> {
        // Header is in the first 8 bytes
        let buf = client.get_range(0, 8).await?;

        let mut reader = Cursor::new(buf);

        // Byte order is in the first 2 bytes
        let byteorder = reader.read_u16::<LittleEndian>()?;

        // Based on byte order, we can parse the remaining 6 bytes
        match TIFFByteOrder::from_u16(byteorder) {
            Some(TIFFByteOrder::LittleEndian) => Ok(CogHeader::parse::<LittleEndian>(
                &mut reader,
                TIFFByteOrder::LittleEndian,
            )?),
            Some(TIFFByteOrder::BigEndian) => Ok(CogHeader::parse::<BigEndian>(
                &mut reader,
                TIFFByteOrder::BigEndian,
            )?),
            None => Err(CogErr::from(Error::new(
                ErrorKind::InvalidData,
                "Invalid TIFF byte order",
            ))),
        }
    }
}

enum_from_primitive! {
    #[repr(u16)]
    #[derive(Debug, Deserialize, Serialize)]
    enum TIFFByteOrder {
        LittleEndian = 0x4949,
        BigEndian    = 0x4d4d,
    }
}
