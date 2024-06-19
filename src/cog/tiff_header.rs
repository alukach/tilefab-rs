use byteorder::{BigEndian, ByteOrder, LittleEndian, ReadBytesExt};
use std::collections::HashMap;
use std::io::{Cursor, Error, ErrorKind, Read, Seek, SeekFrom};

#[derive(Debug)]
pub enum TiffTag {
    ImageWidth(u32),
    ImageLength(u32),
    BitsPerSample(u16),
    Compression(u16),
    PhotometricInterpretation(u16),
    StripOffsets(Vec<u32>),
    SamplesPerPixel(u16),
    RowsPerStrip(u32),
    StripByteCounts(Vec<u32>),
    XResolution(f64),
    YResolution(f64),
    ResolutionUnit(u16),
    Software(String),
    DateTime(String),
    ModelPixelScale(Vec<f64>),
    ModelTiepoint(Vec<f64>),
    GeoKeyDirectory(Vec<u16>),
    GeoDoubleParams(Vec<f64>),
    GeoAsciiParams(String),
    Unknown(u16, Vec<u8>), // Tag ID and raw bytes
}

impl TiffTag {
    fn parse(
        tag: u16,
        field_type: u16,
        count: u32,
        offset: u32,
        endian: bool,
        data: &[u8],
    ) -> Result<TiffTag, Error> {
        let mut cursor = Cursor::new(data);
        cursor.seek(SeekFrom::Start(offset as u64))?;

        match tag {
            256 => Ok(TiffTag::ImageWidth(cursor.read_u32::<LittleEndian>()?)),
            257 => Ok(TiffTag::ImageLength(cursor.read_u32::<LittleEndian>()?)),
            258 => Ok(TiffTag::BitsPerSample(cursor.read_u16::<LittleEndian>()?)),
            259 => Ok(TiffTag::Compression(cursor.read_u16::<LittleEndian>()?)),
            262 => Ok(TiffTag::PhotometricInterpretation(
                cursor.read_u16::<LittleEndian>()?,
            )),
            273 => {
                let mut offsets = Vec::new();
                for _ in 0..count {
                    offsets.push(cursor.read_u32::<LittleEndian>()?);
                }
                Ok(TiffTag::StripOffsets(offsets))
            }
            277 => Ok(TiffTag::SamplesPerPixel(cursor.read_u16::<LittleEndian>()?)),
            278 => Ok(TiffTag::RowsPerStrip(cursor.read_u32::<LittleEndian>()?)),
            279 => {
                let mut byte_counts = Vec::new();
                for _ in 0..count {
                    byte_counts.push(cursor.read_u32::<LittleEndian>()?);
                }
                Ok(TiffTag::StripByteCounts(byte_counts))
            }
            282 => Ok(TiffTag::XResolution(cursor.read_f64::<LittleEndian>()?)),
            283 => Ok(TiffTag::YResolution(cursor.read_f64::<LittleEndian>()?)),
            296 => Ok(TiffTag::ResolutionUnit(cursor.read_u16::<LittleEndian>()?)),
            305 => {
                let mut string_data = vec![0; count as usize];
                cursor.read_exact(&mut string_data)?;
                Ok(TiffTag::Software(String::from_utf8(string_data).map_err(
                    |e| Error::new(ErrorKind::InvalidData, "Invalid software tag data"),
                )?))
            }
            306 => {
                let mut string_data = vec![0; count as usize];
                cursor.read_exact(&mut string_data)?;
                Ok(TiffTag::DateTime(String::from_utf8(string_data).map_err(
                    |e| Error::new(ErrorKind::InvalidData, "Invalid datetime tag data"),
                )?))
            }
            33550 => {
                let mut scale = Vec::new();
                for _ in 0..(count / 3) {
                    scale.push(cursor.read_f64::<LittleEndian>()?);
                }
                Ok(TiffTag::ModelPixelScale(scale))
            }
            33922 => {
                let mut tiepoints = Vec::new();
                for _ in 0..(count / 6) {
                    tiepoints.push(cursor.read_f64::<LittleEndian>()?);
                }
                Ok(TiffTag::ModelTiepoint(tiepoints))
            }
            34735 => {
                let mut keys = Vec::new();
                for _ in 0..count {
                    keys.push(cursor.read_u16::<LittleEndian>()?);
                }
                Ok(TiffTag::GeoKeyDirectory(keys))
            }
            34736 => {
                let mut doubles = Vec::new();
                for _ in 0..count {
                    doubles.push(cursor.read_f64::<LittleEndian>()?);
                }
                Ok(TiffTag::GeoDoubleParams(doubles))
            }
            34737 => {
                let mut string_data = vec![0; count as usize];
                cursor.read_exact(&mut string_data)?;
                Ok(TiffTag::GeoAsciiParams(
                    String::from_utf8(string_data).map_err(|e| {
                        Error::new(ErrorKind::InvalidData, "Invalid GeoAsciiParams tag data")
                    })?,
                ))
            }
            _ => {
                let mut raw_data = vec![0; (count * (field_type_size(field_type) as u32)) as usize];
                cursor.read_exact(&mut raw_data)?;
                Ok(TiffTag::Unknown(tag, raw_data))
            }
        }
    }
}

// Function to get the size of each field type
fn field_type_size(field_type: u16) -> usize {
    match field_type {
        1 => 1,  // BYTE
        2 => 1,  // ASCII
        3 => 2,  // SHORT
        4 => 4,  // LONG
        5 => 8,  // RATIONAL
        6 => 1,  // SBYTE
        7 => 1,  // UNDEFINED
        8 => 2,  // SSHORT
        9 => 4,  // SLONG
        10 => 8, // SRATIONAL
        11 => 4, // FLOAT
        12 => 8, // DOUBLE
        _ => 1,
    }
}

#[derive(Debug)]
pub struct TiffHeader {
    pub endian: bool,
    pub ifd_offset: u64,
    pub tags: HashMap<u16, TiffTag>,
}
impl TiffHeader {
    pub fn new(buffer: &Vec<u8>) -> Result<Self, Error> {
        let mut cursor = Cursor::new(buffer);

        // Read endian
        let mut endian_bytes = [0; 2];
        cursor.read_exact(&mut endian_bytes)?;
        let endian = match &endian_bytes {
            b"II" => true,  // Little Endian
            b"MM" => false, // Big Endian
            _ => return Err(Error::new(ErrorKind::InvalidData, "Invalid TIFF endian")),
        };

        // Read & verify version
        let mut version_bytes = [0; 2];
        cursor.read_exact(&mut version_bytes)?;
        let version = match endian {
            true => LittleEndian::read_u16(&version_bytes),
            false => BigEndian::read_u16(&version_bytes),
        };
        if version != 42 {
            return Err(Error::new(ErrorKind::InvalidData, "Invalid TIFF version"));
        }

        // Read IFD offset
        let mut offset_bytes = [0; 4];
        cursor.read_exact(&mut offset_bytes)?;
        let ifd_offset = match endian {
            true => LittleEndian::read_u32(&offset_bytes),
            false => BigEndian::read_u32(&offset_bytes),
        } as u64;

        // Move to the IFD offset
        cursor.seek(SeekFrom::Start(ifd_offset))?;

        // Read the number of directory entries
        let mut num_entries_bytes = [0; 2];
        cursor.read_exact(&mut num_entries_bytes)?;
        let num_entries = match endian {
            true => LittleEndian::read_u16(&num_entries_bytes),
            false => BigEndian::read_u16(&num_entries_bytes),
        };

        // Read each IFD entry
        let mut tags: HashMap<u16, TiffTag, _> = HashMap::new();
        for _ in 0..num_entries {
            let mut tag_bytes = [0; 12];
            cursor.read_exact(&mut tag_bytes)?;
            let tag_id = LittleEndian::read_u16(&tag_bytes[0..2]);
            let field_type = LittleEndian::read_u16(&tag_bytes[2..4]);
            let count = LittleEndian::read_u32(&tag_bytes[4..8]);
            let offset = LittleEndian::read_u32(&tag_bytes[8..12]);

            let tag_data = {
                let current_pos = cursor.seek(SeekFrom::Current(0))?;
                cursor.seek(SeekFrom::Start(offset as u64))?;
                let mut data = vec![0; (count * field_type_size(field_type) as u32) as usize];
                cursor.read_exact(&mut data)?;
            //     cursor.seek(SeekFrom::Start(current_pos))?;
            //     data
            };

            // let tag = TiffTag::parse(tag_id, field_type, count, offset, true, &tag_data)?;
            // tags.insert(tag_id, tag);
            break; // TODO: Remove this break
        }

        Ok(Self {
            endian,
            ifd_offset,
            tags,
        })
    }
}
