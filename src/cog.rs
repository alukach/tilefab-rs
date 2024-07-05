use crate::errors::CogErr;
use byteorder::{BigEndian, ByteOrder, LittleEndian, ReadBytesExt};
use http_range_client::BufferedHttpRangeClient;
use serde::{Deserialize, Serialize};
use std::io::{Error, ErrorKind, Read};
use worker as cf;

#[derive(Debug, Deserialize, Serialize)]
pub struct Cog {
    pub header: CogHeader,
    pub ifds: Vec<IFD>,
}

impl Cog {
    pub async fn new(client: &mut BufferedHttpRangeClient) -> Result<Self, CogErr> {
        // Header is in the first 8 bytes
        let buf = client.get_range(0, 8).await?;
        let header = CogHeader::new(buf)?;
        cf::console_log!("Header: {:?}", header);

        // Parse IFDs
        let offset = header.ifd_offset as usize;
        let mut ifds: Vec<IFD> = vec![];

        loop {
            let ifd = match header.byteorder {
                TIFFByteOrder::LittleEndian => IFD::parse::<LittleEndian>(client, offset).await?,
                TIFFByteOrder::BigEndian => IFD::parse::<BigEndian>(client, offset).await?,
            };
            // 2-byte count of the number of directory entries (i.e. the number of fields)
            ifds.push(ifd);

            // a 4-byte offset of the next IFD (or 0 if none)
            break;
        }

        Ok(Self { header, ifds })
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CogHeader {
    byteorder: TIFFByteOrder,
    ifd_offset: u32,
}

impl CogHeader {
    pub fn new(mut reader: impl Read) -> Result<Self, CogErr> {
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
        reader: &mut impl Read,
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

#[derive(Debug, Deserialize, Serialize)]
pub struct IFD {
    pub count: u16,
    pub entries: Vec<IFDEntry>,
}

impl IFD {
    async fn parse<T: ByteOrder>(
        client: &mut BufferedHttpRangeClient,
        offset: usize,
    ) -> Result<Self, CogErr> {
        let mut entry_count_reader = client.get_range(offset, 2).await?;
        let entry_count = entry_count_reader
            .read_u16::<T>()
            .expect("slice with incorrect length");
        cf::console_log!("Num fields: {:?}", entry_count);

        let fields_bytes = client
            .get_range(offset + 2, ((entry_count * 12) + 4) as usize)
            .await?;

        // a sequence of 12-byte field entries
        let mut entries: Vec<IFDEntry> = vec![];
        for _ in 0..entry_count {
            let ifd_entry = IFDEntry::parse::<T>(fields_bytes)?;
            entries.push(ifd_entry);
        }
        Ok(Self { count: 0, entries })
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct IFDEntry {
    tag: u16,
    field_type: u16,
    num_values: u32,
    value_offset: u32,
}

impl IFDEntry {
    fn parse<T: ByteOrder>(mut reader: impl Read) -> Result<Self, Error> {
        let tag = reader.read_u16::<T>()?;
        // 2-byte field type
        let field_type = reader.read_u16::<T>()?;
        // 4-byte count of the number of values
        let num_values = reader.read_u32::<T>()?;
        // 4-byte value offset
        let value_offset = reader.read_u32::<T>()?;
        Ok(Self {
            tag,
            field_type,
            num_values,
            value_offset,
        })
    }
}

#[derive(Debug, Deserialize, Serialize)]
enum TIFFByteOrder {
    LittleEndian,
    BigEndian,
}
