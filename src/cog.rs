use crate::errors::CogErr;
use byteorder::{BigEndian, ByteOrder, LittleEndian, ReadBytesExt};
use geotiff::lowlevel::{tag_size, TagValue};
use http_range_client::BufferedHttpRangeClient;
use serde::{Deserialize, Serialize};
use std::{
    io::{Cursor, Error, ErrorKind, Read, Seek},
    vec,
};
use tiff::tags::{Tag, Type};
use tiff::TiffResult;
use tiff::{
    decoder::{
        ifd::Value::{self, *},
        Limits,
    },
    TiffError, TiffFormatError,
};
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
            let Some(tag) = Tag::from_u16(tag_value) else {
                cf::console_debug!(
                    "#{:?}/{:?}: Ignoring entry with unexpected tag value ({:?})",
                    entry_num + 1,
                    entry_count,
                    tag_value
                );
                continue;
                // TODO: Fail on bad tag value
                // Err(CogErr::from(Error::new(
                //     ErrorKind::InvalidData,
                //     format!("Invalid tag {:04X}", tag_value),
                // )))
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
                // Err(CogErr::from(Error::new(
                //     ErrorKind::InvalidData,
                //     format!("Invalid tag type {:04X}", field_type_value),
                // )))
            };
            let value_size = tag_size(&field_type);

            // Bytes 4..7: u32 number of Values of type
            let num_values = fields_bytes.read_u32::<T>()?;
            let tot_size = num_values * value_size;

            // Let's get the value(s) of this tag.
            // let mut values = Vec::with_capacity(num_values as usize);

            // Bytes 8..11: u32 offset in file to Value
            let value_offset = fields_bytes.read_u32::<T>()?;
            let treat_offset_as_value = tot_size <= 4;
            let mut value_data: &[u8] = match treat_offset_as_value {
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

            // for _ in 0..num_values as usize {
            //     let mut buf = vec![0u8; value_size as usize];
            //     value_data.read(&mut buf)?;
            //     let val = Self::vec_to_tag_value::<T>(buf, &field_type)?;
            //     values.push(val);
            // }

            // cf::console_debug!(
            //     "#{:?}/{:?}: {:?}, {:?}",
            //     entry_num + 1,
            //     entry_count,
            //     tag,
            //     values
            // );

            // DEV: Let's test using the tiff module...
            let mut _offset = [0u8; 4];
            T::write_u32(&mut _offset, value_offset);
            let entry = Entry::new(field_type, num_values, _offset);
            let bigtiff = false; // TODO: How to determine this?
            let mut reader = Cursor::new(value_data); // TODO: Build smart reader here...
            let value = entry
                .val::<T>(Limits::default(), bigtiff, &mut reader)
                .or_else(|e| {
                    Err(CogErr::from(Error::new(
                        ErrorKind::InvalidData,
                        format!("Failed to read value for tag {:?}", tag),
                    )))
                })?;

            cf::console_debug!(
                "#{:?}/{:?}: {:?}, {:?}",
                entry_num + 1,
                entry_count,
                tag,
                value
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

// Copied from `tiff` crate: https://github.com/image-rs/image-tiff/blob/b0ba4a6788cc897cd14bd20d93a6472ae1200295/src/decoder/ifd.rs#L305
// Modified to use our reader instead of SmartReader
#[derive(Clone)]
pub struct Entry {
    type_: Type,
    count: u64,
    offset: [u8; 8],
}

impl Entry {
    pub fn new(type_: Type, count: u32, offset: [u8; 4]) -> Entry {
        let mut offset = offset.to_vec();
        offset.append(&mut vec![0; 4]);
        Entry::new_u64(type_, count.into(), offset[..].try_into().unwrap())
    }

    pub fn new_u64(type_: Type, count: u64, offset: [u8; 8]) -> Entry {
        Entry {
            type_,
            count,
            offset,
        }
    }

    pub fn val<T: ByteOrder>(
        &self,
        limits: Limits,
        bigtiff: bool,
        reader: impl Read + Seek,
    ) -> TiffResult<Value> {
        // Case 1: there are no values so we can return immediately.
        if self.count == 0 {
            return Ok(List(Vec::new()));
        }

        let tag_size = match self.type_ {
            Type::BYTE | Type::SBYTE | Type::ASCII | Type::UNDEFINED => 1,
            Type::SHORT | Type::SSHORT => 2,
            Type::LONG | Type::SLONG | Type::FLOAT | Type::IFD => 4,
            Type::LONG8
            | Type::SLONG8
            | Type::DOUBLE
            | Type::RATIONAL
            | Type::SRATIONAL
            | Type::IFD8 => 8,
            _ => return Err(TiffError::FormatError(TiffFormatError::InvalidTag)), // NOTE: Added
        };

        let value_bytes = match self.count.checked_mul(tag_size) {
            Some(n) => n,
            None => {
                return Err(TiffError::LimitsExceeded);
            }
        };

        // Case 2: there is one value.
        if self.count == 1 {
            // 2a: the value is 5-8 bytes and we're in BigTiff mode.
            if bigtiff && value_bytes > 4 && value_bytes <= 8 {
                return Ok(match self.type_ {
                    Type::LONG8 => UnsignedBig(T::read_u64(&self.offset)),
                    Type::SLONG8 => SignedBig(reader.read_i64()?),
                    Type::DOUBLE => Double(reader.read_f64()?),
                    Type::RATIONAL => {
                        let mut r = reader;
                        Rational(r.read_u32()?, r.read_u32()?)
                    }
                    Type::SRATIONAL => {
                        let mut r = reader;
                        SRational(r.read_i32()?, r.read_i32()?)
                    }
                    Type::IFD8 => IfdBig(reader.read_u64()?),
                    Type::BYTE
                    | Type::SBYTE
                    | Type::ASCII
                    | Type::UNDEFINED
                    | Type::SHORT
                    | Type::SSHORT
                    | Type::LONG
                    | Type::SLONG
                    | Type::FLOAT
                    | Type::IFD => unreachable!(),
                    _ => return Err(TiffError::FormatError(TiffFormatError::InvalidTag)), // NOTE: Added
                });
            }

            // 2b: the value is at most 4 bytes or doesn't fit in the offset field.
            return Ok(match self.type_ {
                Type::BYTE => Unsigned(u32::from(self.offset[0])),
                Type::SBYTE => Signed(i32::from(self.offset[0] as i8)),
                Type::UNDEFINED => Byte(self.offset[0]),
                Type::SHORT => Unsigned(u32::from(reader.read_u16()?)),
                Type::SSHORT => Signed(i32::from(reader.read_i16()?)),
                Type::LONG => Unsigned(reader.read_u32()?),
                Type::SLONG => Signed(reader.read_i32()?),
                Type::FLOAT => Float(reader.read_f32()?),
                Type::ASCII => {
                    if self.offset[0] == 0 {
                        Ascii("".to_string())
                    } else {
                        return Err(TiffError::FormatError(TiffFormatError::InvalidTag));
                    }
                }
                Type::LONG8 => {
                    reader.goto_offset(reader.read_u32()?.into())?;
                    UnsignedBig(reader.read_u64()?)
                }
                Type::SLONG8 => {
                    reader.goto_offset(reader.read_u32()?.into())?;
                    SignedBig(reader.read_i64()?)
                }
                Type::DOUBLE => {
                    reader.goto_offset(reader.read_u32()?.into())?;
                    Double(reader.read_f64()?)
                }
                Type::RATIONAL => {
                    reader.goto_offset(reader.read_u32()?.into())?;
                    Rational(reader.read_u32()?, reader.read_u32()?)
                }
                Type::SRATIONAL => {
                    reader.goto_offset(reader.read_u32()?.into())?;
                    SRational(reader.read_i32()?, reader.read_i32()?)
                }
                Type::IFD => Ifd(reader.read_u32()?),
                Type::IFD8 => {
                    reader.goto_offset(reader.read_u32()?.into())?;
                    IfdBig(reader.read_u64()?)
                }
                _ => return Err(TiffError::FormatError(TiffFormatError::InvalidTag)), // NOTE: Added
            });
        }

        // Case 3: There is more than one value, but it fits in the offset field.
        if value_bytes <= 4 || bigtiff && value_bytes <= 8 {
            match self.type_ {
                Type::BYTE => return offset_to_bytes(self.count as usize, self),
                Type::SBYTE => return offset_to_sbytes(self.count as usize, self),
                Type::ASCII => {
                    let mut buf = vec![0; self.count as usize];
                    reader.read_exact(&mut buf)?;
                    if buf.is_ascii() && buf.ends_with(&[0]) {
                        let v = str::from_utf8(&buf)?;
                        let v = v.trim_matches(char::from(0));
                        return Ok(Ascii(v.into()));
                    } else {
                        return Err(TiffError::FormatError(TiffFormatError::InvalidTag));
                    }
                }
                Type::UNDEFINED => {
                    return Ok(List(
                        self.offset[0..self.count as usize]
                            .iter()
                            .map(|&b| Byte(b))
                            .collect(),
                    ));
                }
                Type::SHORT => {
                    let mut r = reader;
                    let mut v = Vec::new();
                    for _ in 0..self.count {
                        v.push(Short(r.read_u16()?));
                    }
                    return Ok(List(v));
                }
                Type::SSHORT => {
                    let mut r = reader;
                    let mut v = Vec::new();
                    for _ in 0..self.count {
                        v.push(Signed(i32::from(r.read_i16()?)));
                    }
                    return Ok(List(v));
                }
                Type::LONG => {
                    let mut r = reader;
                    let mut v = Vec::new();
                    for _ in 0..self.count {
                        v.push(Unsigned(r.read_u32()?));
                    }
                    return Ok(List(v));
                }
                Type::SLONG => {
                    let mut r = reader;
                    let mut v = Vec::new();
                    for _ in 0..self.count {
                        v.push(Signed(r.read_i32()?));
                    }
                    return Ok(List(v));
                }
                Type::FLOAT => {
                    let mut r = reader;
                    let mut v = Vec::new();
                    for _ in 0..self.count {
                        v.push(Float(r.read_f32()?));
                    }
                    return Ok(List(v));
                }
                Type::IFD => {
                    let mut r = reader;
                    let mut v = Vec::new();
                    for _ in 0..self.count {
                        v.push(Ifd(r.read_u32()?));
                    }
                    return Ok(List(v));
                }
                Type::LONG8
                | Type::SLONG8
                | Type::RATIONAL
                | Type::SRATIONAL
                | Type::DOUBLE
                | Type::IFD8 => {
                    unreachable!()
                }
                _ => return Err(TiffError::FormatError(TiffFormatError::InvalidTag)), // NOTE: Added
            }
        }

        // Case 4: there is more than one value, and it doesn't fit in the offset field.
        match self.type_ {
            // TODO check if this could give wrong results
            // at a different endianess of file/computer.
            Type::BYTE => self.decode_offset(self.count, bo, bigtiff, limits, reader, |reader| {
                let mut buf = [0; 1];
                reader.read_exact(&mut buf)?;
                Ok(UnsignedBig(u64::from(buf[0])))
            }),
            Type::SBYTE => self.decode_offset(self.count, bo, bigtiff, limits, reader, |reader| {
                Ok(SignedBig(i64::from(reader.read_i8()?)))
            }),
            Type::SHORT => self.decode_offset(self.count, bo, bigtiff, limits, reader, |reader| {
                Ok(UnsignedBig(u64::from(reader.read_u16()?)))
            }),
            Type::SSHORT => self.decode_offset(self.count, bo, bigtiff, limits, reader, |reader| {
                Ok(SignedBig(i64::from(reader.read_i16()?)))
            }),
            Type::LONG => self.decode_offset(self.count, bo, bigtiff, limits, reader, |reader| {
                Ok(Unsigned(reader.read_u32()?))
            }),
            Type::SLONG => self.decode_offset(self.count, bo, bigtiff, limits, reader, |reader| {
                Ok(Signed(reader.read_i32()?))
            }),
            Type::FLOAT => self.decode_offset(self.count, bo, bigtiff, limits, reader, |reader| {
                Ok(Float(reader.read_f32()?))
            }),
            Type::DOUBLE => self.decode_offset(self.count, bo, bigtiff, limits, reader, |reader| {
                Ok(Double(reader.read_f64()?))
            }),
            Type::RATIONAL => {
                self.decode_offset(self.count, bo, bigtiff, limits, reader, |reader| {
                    Ok(Rational(reader.read_u32()?, reader.read_u32()?))
                })
            }
            Type::SRATIONAL => {
                self.decode_offset(self.count, bo, bigtiff, limits, reader, |reader| {
                    Ok(SRational(reader.read_i32()?, reader.read_i32()?))
                })
            }
            Type::LONG8 => self.decode_offset(self.count, bo, bigtiff, limits, reader, |reader| {
                Ok(UnsignedBig(reader.read_u64()?))
            }),
            Type::SLONG8 => self.decode_offset(self.count, bo, bigtiff, limits, reader, |reader| {
                Ok(SignedBig(reader.read_i64()?))
            }),
            Type::IFD => self.decode_offset(self.count, bo, bigtiff, limits, reader, |reader| {
                Ok(Ifd(reader.read_u32()?))
            }),
            Type::IFD8 => self.decode_offset(self.count, bo, bigtiff, limits, reader, |reader| {
                Ok(IfdBig(reader.read_u64()?))
            }),
            Type::UNDEFINED => {
                self.decode_offset(self.count, bo, bigtiff, limits, reader, |reader| {
                    let mut buf = [0; 1];
                    reader.read_exact(&mut buf)?;
                    Ok(Byte(buf[0]))
                })
            }
            Type::ASCII => {
                let n = usize::try_from(self.count)?;
                if n > limits.decoding_buffer_size {
                    return Err(TiffError::LimitsExceeded);
                }

                if bigtiff {
                    reader.goto_offset(reader.read_u64()?)?
                } else {
                    reader.goto_offset(reader.read_u32()?.into())?
                }

                let mut out = vec![0; n];
                reader.read_exact(&mut out)?;
                // Strings may be null-terminated, so we trim anything downstream of the null byte
                if let Some(first) = out.iter().position(|&b| b == 0) {
                    out.truncate(first);
                }
                Ok(Ascii(String::from_utf8(out)?))
            }
            _ => return Err(TiffError::FormatError(TiffFormatError::InvalidTag)), // NOTE: Added
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
enum TIFFByteOrder {
    LittleEndian,
    BigEndian,
}

// #[derive(Debug, Deserialize, Serialize)]
// struct Tag {
//     // tag_value: u16,
//     // field_type_value: u16,
//     // num_values: u32,
//     // value_offset: u32,
// }
