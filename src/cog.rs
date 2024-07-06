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
        let mut offset = header.ifd_offset;
        let mut ifds: Vec<IFD> = vec![];

        loop {
            let (ifd, next_ifd_offset) = match header.byteorder {
                TIFFByteOrder::LittleEndian => {
                    IFD::parse::<LittleEndian>(client, offset as usize).await?
                }
                TIFFByteOrder::BigEndian => {
                    IFD::parse::<BigEndian>(client, offset as usize).await?
                }
            };
            ifds.push(ifd);

            if (next_ifd_offset as usize) == 0 {
                cf::console_debug!("No more IFDs");
                break;
            }
            offset = next_ifd_offset;
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
    /**
     * The 8-byte TIFF file header contains the following information:
     *
     * Bytes  | Description
     * ---------------------
     * 0-1    | The byte order used within the file. Legal values are:“II”(4949.H)“MM” (4D4D.H).
     *        | In the “II” format, byte order is always from the least significant byte to the
     *        | most significant byte, for both 16-bit and 32-bit integers This is called little-endian
     *        | byte order. In the “MM” format, byte order is always from most significant to least
     *        | significant, for both 16-bit and 32-bit integers. This is called big-endian byte order.
     * 2-3    | An arbitrary but carefully chosen number (42) that further identifies the file as a
     *        | TIFF file.The byte order depends on the value of Bytes 0-1.
     * 4-7    | The offset (in bytes) of the first IFD. The directory may be at any location in the file
     *        | after the header but must begin on a word boundary. In particular, an Image File Directory
     *        | may follow the image data it describes. Readers must follow the pointers wherever they
     *        | may lead. The term byte offset is always used in this document to refer to a location with
     *        | respect to the beginning of the TIFF file. The first byte of the file has an offset of 0.
     */
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
    /**
     * An IFD contains information about the image as well as pointers to the actual image data..
     * It consists of a 2-byte count of the number of directory entries (i.e. the number of fields),
     * followed by a sequence of 12-byte field entries, followed by a 4-byte offset of the next IFD
     * (or 0 if none). There must be at least 1 IFD in a TIFF file and each IFD must have at least
     * one entry.
     */
    async fn parse<T: ByteOrder>(
        client: &mut BufferedHttpRangeClient,
        offset: usize,
    ) -> Result<(Self, u32), CogErr> {
        cf::console_debug!("Processing IFD at offset {:?}", offset);

        // a 2-byte count of the number of directory entries
        let mut entry_count_reader = client.get_range(offset, 2).await?;
        let entry_count = entry_count_reader
            .read_u16::<T>()
            .expect("slice with incorrect length");
        cf::console_log!("Num fields: {:?}", entry_count);

        // a sequence of 12-byte field entries
        let mut entries: Vec<IFDEntry> = Vec::with_capacity(entry_count as usize);
        for entry_num in 0..entry_count as usize {
            let fields_bytes = client
                // Take our IFD offset, add 2 bytes for the entry count, and then add 12 bytes for each entry we've processed so far
                .get_range(offset + 2 + (entry_num * 12), 12 as usize)
                .await?;

            let ifd_entry = IFDEntry::parse::<T>(fields_bytes)?;

            entries.push(ifd_entry);
        }

        // a 4-byte offset of the next IFD (or 0 if none)
        let mut next_ifd_offset_reader = client
            .get_range(offset + 2 + (entry_count as usize * 12), 4)
            .await?;
        let next_ifd_offset = next_ifd_offset_reader
            .read_u32::<T>()
            .expect("slice with incorrect length");

        Ok((
            Self {
                count: entry_count,
                entries,
            },
            next_ifd_offset,
        ))
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct IFDEntry {
    tag: u16,
    field_type: u16,
    num_values: u32,
    value_offset: u32,
}

impl IFDEntry {
    /**
     * Each 12-byte IFD Entry is in the following format.
     *
     * Bytes  | Description
     * ---------------------
     * 0-1	  | The Tag that identifies the field
     * 2-3	  | The field type
     * 4-7	  | Count of the indicated type
     * 8-11	  | The Value Offset, the file offset (in bytes) of the Value for the field. The Value is
     *        | expected to begin on a word boundary; the correspond-ing Value Offset will thus be an
     *        | even number. This file offset may point anywhere in the file, even after the image data.
     *
     * A TIFF field is a logical entity consisting of TIFF tag and its value. This logical concept
     * is implemented as an IFD Entry, plus the actual value if it doesn’t fit into the value/offset
     * part, the last 4 bytes of the IFD Entry. The terms TIFF field and IFD entry are interchangeable
     * in most contexts.
     */
    fn parse<T: ByteOrder>(mut reader: impl Read) -> Result<Self, Error> {
        // TODO: Use `tiff` or `geotiff` to parse tag data
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
