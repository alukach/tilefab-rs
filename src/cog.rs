use crate::errors::CogErr;
use byteorder::{BigEndian, ByteOrder, LittleEndian, ReadBytesExt};
use geotiff::{
    geotiff::decode_tag,
    lowlevel::{tag_size, TIFFTag, TagValue},
};
use http_range_client::BufferedHttpRangeClient;
use serde::{Deserialize, Serialize};
use std::io::{Error, ErrorKind, Read};
use tiff::tags::Type;
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

            if next_ifd_offset == 0 {
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
    // pub entries: Vec<IFDEntry>,
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
        // let mut entries: Vec<IFDEntry> = Vec::with_capacity(entry_count as usize);
        for entry_num in 0..entry_count as usize {
            let mut fields_bytes = client
                // Take our IFD offset, add 2 bytes for the entry count, and then add 12 bytes for each entry we've processed so far
                .get_range(offset + 2 + (entry_num * 12), 12 as usize)
                .await?;

            // Each 12-byte IFD Entry is in the following format.
            //
            // Bytes  | Description
            // ---------------------
            // 0-1	  | The Tag that identifies the field
            // 2-3	  | The field type
            // 4-7	  | Count of the indicated type
            // 8-11	  | The Value Offset, the file offset (in bytes) of the Value for the field. The Value is
            //        | expected to begin on a word boundary; the correspond-ing Value Offset will thus be an
            //        | even number. This file offset may point anywhere in the file, even after the image data.
            //
            // A TIFF field is a logical entity consisting of TIFF tag and its value. This logical concept
            // is implemented as an IFD Entry, plus the actual value if it doesn’t fit into the value/offset
            // part, the last 4 bytes of the IFD Entry. The terms TIFF field and IFD entry are interchangeable
            // in most contexts.

            // TODO: Use `tiff` or `geotiff` to parse tag data

            // Bytes 0..1: u16 tag ID
            let tag_value = fields_bytes.read_u16::<T>()?;
            let Some(tag) = decode_tag(tag_value) else {
                // https://www.loc.gov/preservation/digital/formats/content/tiff_tags.shtml
                cf::console_warn!(
                    "#{:?}/{:?}: Ignoring entry with unexpected tag value ({:?})",
                    entry_num + 1,
                    entry_count,
                    tag_value
                );
                continue;
                // TODO: Fail on bad tag type
                // return Err(CogErr::from(Error::new(
                //     ErrorKind::InvalidData,
                //     format!("Invalid tag {:04X}", tag_value),
                // )));
            };

            // Bytes 2..3: u16 field Type
            let field_type_value = fields_bytes.read_u16::<T>()?;
            let Some(field_type) = Type::from_u16(field_type_value) else {
                cf::console_warn!(
                    "#{:?}/{:?}: Ignoring entry with unexpected field type value ({:?})",
                    entry_num + 1,
                    entry_count,
                    field_type_value
                );
                continue;
                // TODO: Fail on bad field type
                // return Err(CogErr::from(Error::new(
                //     ErrorKind::InvalidData,
                //     format!("Invalid tag type {:04X}", field_type_value),
                // )));
            };
            let value_size = tag_size(&field_type);

            // Bytes 4..7: u32 number of Values of type
            let num_values = fields_bytes.read_u32::<T>()?;
            let tot_size = num_values * value_size;

            // Let's get the value(s) of this tag.
            let mut values = Vec::with_capacity(num_values as usize);

            // Bytes 8..11: u32 offset in file to Value
            let value_offset = fields_bytes.read_u32::<T>()?;
            let mut value_data: &[u8] = match tot_size <= 4 {
                true => {
                    // NOTE: If the value is <= 4 bytes, the value offset is the value itself. I can't
                    // find this mentioned in the spec, but all reference implementations do this.
                    // cf::console_debug!(
                    //     "Total size of {:?} (<=4), treating value offset as value",
                    //     tot_size,
                    // );
                    let mut buf = [0u8; 4];
                    T::write_u32(&mut buf, value_offset);
                    &buf.to_vec()
                }
                false => {
                    client
                        .get_range((value_offset + tot_size) as usize, value_size as usize)
                        .await?
                }
            };
            for _ in 0..num_values as usize {
                let mut buf = [0u8; 4];
                value_data.read(&mut buf)?;
                let val = Self::vec_to_tag_value::<T>(buf.to_vec(), &field_type)?;
                values.push(val);
            }

            cf::console_debug!(
                "#{:?}/{:?}: {:?}, {:?}",
                entry_num + 1,
                entry_count,
                tag,
                values
            );

            // let tag = ifd_entry.as_tag::<T>(client).await?;
            // cf::console_log!("Tag: {:?}", tag);

            // entries.push(ifd_entry);
        }

        // cf::console_log!("IFD entries: {:?}", entries);

        // a 4-byte offset of the next IFD (or 0 if none)
        let mut next_ifd_offset_reader = client
            .get_range(offset + 2 + (entry_count as usize * 12), 4)
            .await?;
        let next_ifd_offset: u32 = next_ifd_offset_reader
            .read_u32::<T>()
            .expect("slice with incorrect length");

        Ok((
            Self {
                count: entry_count,
                // entries,
            },
            next_ifd_offset,
        ))
    }

    /// Converts a Vec<u8> into a TagValue, depending on the type of the tag. In the TIFF file
    /// format, each tag type indicates which value it stores (e.g., a byte, ascii, or long value).
    /// This means that the tag values have to be read taking the tag type into consideration.
    fn vec_to_tag_value<Endian: ByteOrder>(vec: Vec<u8>, tpe: &Type) -> Result<TagValue, CogErr> {
        let len = vec.len();
        match tpe {
            Type::BYTE => Ok(TagValue::ByteValue(vec[0])),
            Type::ASCII => Ok(TagValue::AsciiValue(
                String::from_utf8_lossy(&vec).to_string(),
            )),
            Type::SHORT => Ok(TagValue::ShortValue(Endian::read_u16(&vec[..]))),
            Type::LONG => Ok(TagValue::LongValue(Endian::read_u32(&vec[..]))),
            Type::RATIONAL => {
                cf::console_debug!("Parsing {:?} ({:?}). This will likely fail.", vec, tpe);
                Ok(TagValue::RationalValue((
                    // TODO: This fails...
                    Endian::read_u32(&vec[..(len / 2)]),
                    Endian::read_u32(&vec[(len / 2)..]),
                )))
            }
            Type::SBYTE => Ok(TagValue::SignedByteValue(vec[0] as i8)),
            Type::UNDEFINED => Ok(TagValue::ByteValue(0)),
            Type::SSHORT => Ok(TagValue::SignedShortValue(Endian::read_i16(&vec[..]))),
            Type::SLONG => Ok(TagValue::SignedLongValue(Endian::read_i32(&vec[..]))),
            Type::SRATIONAL => Ok(TagValue::SignedRationalValue((
                Endian::read_i32(&vec[..(len / 2)]),
                Endian::read_i32(&vec[(len / 2)..]),
            ))),
            Type::FLOAT => Ok(TagValue::FloatValue(Endian::read_f32(&vec[..]))),
            Type::DOUBLE => Ok(TagValue::DoubleValue(Endian::read_f64(&vec[..]))),
            // Type::IFD => ,
            // Type::LONG8 => ,
            // Type::SLONG8 => ,
            // Type::IFD8 => ,
            _ => Err(CogErr::from(Error::new(
                ErrorKind::InvalidData,
                format!("Invalid field type {:?}", tpe),
            ))),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
enum TIFFByteOrder {
    LittleEndian,
    BigEndian,
}

#[derive(Debug, Deserialize, Serialize)]
struct Tag {
    // tag_value: u16,
    // field_type_value: u16,
    // num_values: u32,
    // value_offset: u32,
}
