use crate::errors::CogErr;
use byteorder::{BigEndian, ByteOrder, LittleEndian, ReadBytesExt};
use http_range_client::BufferedHttpRangeClient;
use serde::{Deserialize, Serialize};
use std::io::{Cursor, Error, ErrorKind, Read};
use worker as cf;

pub struct Cog {
    pub header: CogHeader,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CogHeader {
    byteorder: TIFFByteOrder,
    ifd_offset: u32,
}

impl CogHeader {
    pub fn new(buf: &[u8]) -> Result<Self, CogErr> {
        if !(buf.len() >= 8) {
            return Err(CogErr::Io(std::io::Error::new(
                ErrorKind::InvalidData,
                "Invalid header length",
            )));
        }

        let mut reader = Cursor::new(buf);

        // Byte order is in the first 2 bytes
        let mut byteorder = [0; 2];
        reader.read_exact(&mut byteorder)?;

        // Parse header based on byte order
        match &byteorder {
            b"II" => Ok(CogHeader::parse::<LittleEndian>(
                &mut reader,
                TIFFByteOrder::LittleEndian,
            )?),
            b"MM" => Ok(CogHeader::parse::<BigEndian>(
                &mut reader,
                TIFFByteOrder::BigEndian,
            )?),
            _ => Err(CogErr::from(Error::new(
                ErrorKind::InvalidData,
                "Invalid TIFF byte order",
            ))),
        }
    }

    fn parse<T: ByteOrder>(
        reader: &mut Cursor<&[u8]>,
        byteorder: TIFFByteOrder,
    ) -> Result<Self, CogErr> {
        let magic = reader.read_u16::<T>()?;
        if magic != 42 {
            return Err(CogErr::Io(std::io::Error::new(
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
        cf::console_log!("Header: {:?}", header);
        Ok(Self { header })
    }

    pub async fn fetch_header(mut client: BufferedHttpRangeClient) -> Result<CogHeader, CogErr> {
        // Header is in the first 8 bytes
        let buf = client.get_range(0, 8).await?;
        cf::console_log!("Fetched {:?} bytes for the header", buf.len());
        CogHeader::new(&buf)
    }
}

#[derive(Debug, Deserialize, Serialize)]
enum TIFFByteOrder {
    LittleEndian,
    BigEndian,
}
