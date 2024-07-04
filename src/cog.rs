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
    pub fn parse<T: ByteOrder>(
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

enum_from_primitive! {
    #[repr(u16)]
    #[derive(Debug, Deserialize, Serialize)]
    pub enum TIFFByteOrder {
        LittleEndian = 0x4949,
        BigEndian    = 0x4d4d,
    }
}

impl Cog {
    pub async fn new(client: BufferedHttpRangeClient) -> Result<Self, CogErr> {
        let header = Self::fetch_header(client).await?;
        Ok(Self { header })
    }

    pub async fn fetch_header(mut client: BufferedHttpRangeClient) -> Result<CogHeader, CogErr> {
        let buf = client.get_range(0, 8).await?;
        let mut reader = Cursor::new(buf);
        let byteorder = Self::read_byte_order(&mut reader).unwrap();
        match byteorder {
            TIFFByteOrder::LittleEndian => {
                Ok(CogHeader::parse::<LittleEndian>(&mut reader, byteorder)?)
            }
            TIFFByteOrder::BigEndian => Ok(CogHeader::parse::<BigEndian>(&mut reader, byteorder)?),
        }
    }

    /// Helper function to read the byte order, one of `LittleEndian` or `BigEndian`.
    pub fn read_byte_order(reader: &mut Cursor<&[u8]>) -> Result<TIFFByteOrder, Error> {
        // Bytes 0-1: "II" or "MM"
        // Read and validate ByteOrder
        match TIFFByteOrder::from_u16(reader.read_u16::<LittleEndian>()?) {
            Some(TIFFByteOrder::LittleEndian) => Ok(TIFFByteOrder::LittleEndian),
            Some(TIFFByteOrder::BigEndian) => Ok(TIFFByteOrder::BigEndian),
            None => Err(Error::new(
                ErrorKind::InvalidData,
                "Invalid TIFF byte order",
            )),
        }
    }
}

pub enum CogErr {
    HttpError(http_range_client::HttpError),
    IoError(std::io::Error),
}

impl From<std::io::Error> for CogErr {
    fn from(e: std::io::Error) -> Self {
        Self::IoError(e)
    }
}

impl From<http_range_client::HttpError> for CogErr {
    fn from(e: http_range_client::HttpError) -> Self {
        Self::HttpError(e)
    }
}

impl std::fmt::Display for CogErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The `f` value implements the `Write` trait, which is what the
        // write! macro is expecting. Note that this formatting ignores the
        // various flags provided to format strings.
        write!(f, "ERR")
    }
}
