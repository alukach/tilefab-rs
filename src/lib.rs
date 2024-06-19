use byteorder::{BigEndian, LittleEndian};
use geotiff::lowlevel::TIFFByteOrder;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, io::Cursor};
use worker as cf;

pub mod bounds;
pub mod cog;
pub mod tile;

#[derive(Debug, Deserialize, Serialize)]
struct GenericResponse {
    message: String,
}

#[cf::event(fetch)]
async fn main(req: cf::Request, env: cf::Env, _ctx: cf::Context) -> cf::Result<cf::Response> {
    cf::Router::new()
        .get_async("/", hello)
        .get_async("/:z/:x/:y", get_tile)
        .run(req, env)
        .await
}

pub async fn hello(_: cf::Request, _ctx: cf::RouteContext<()>) -> cf::Result<cf::Response> {
    cf::Response::from_json(&GenericResponse {
        message: "Just another tile server.".to_string(),
    })
}

pub async fn get_tile(req: cf::Request, ctx: cf::RouteContext<()>) -> cf::Result<cf::Response> {
    // Parse tile from path parameters
    let tile = match tile::Tile::new(
        ctx.param("z").unwrap_or(&String::from("")),
        ctx.param("x").unwrap_or(&String::from("")),
        ctx.param("y").unwrap_or(&String::from("")),
    ) {
        Ok(tile) => tile,
        Err(e) => return cf::Response::error(format!("Invalid tile path: {}", e), 400),
    };

    // Retrieve src query parameter
    let query_params: HashMap<String, String> = match req.url() {
        Ok(url) => url.query_pairs().into_owned().collect(),
        Err(_) => return cf::Response::error("Failed to parse URL", 400),
    };
    let src = match query_params.get("src") {
        Some(src) => src,
        None => return cf::Response::error("src query parameter is required", 400),
    };

    let cog = cog::COG {
        src: src.to_string(),
    };

    // Fetch COG header
    /**
     * No magic number to know how big the header will be.
     * GDAL reads first 16KB, will request 2x data for each subsequent page, up to max 2MB.
     * Consider making page size scale-up configurable via query parameter.
     */
    let mut cog_header = match cog.fetch_header(102300).await {
        Ok(body) => body,
        Err(e) => return cf::Response::error(format!("Failed to fetch COG: {}", e), 503),
    };

    let mut reader = Cursor::new(&mut cog_header);

    let tiff_reader = geotiff::reader::TIFFReader {};
    let byte_order = tiff_reader.read_byte_order(&mut reader)?;
    let ifd = match byte_order {
        TIFFByteOrder::LittleEndian => {
            tiff_reader.read_magic::<LittleEndian>(&mut reader)?;
            let ifd_offset = tiff_reader.read_ifd_offset::<LittleEndian>(&mut reader)?;
            match tiff_reader.read_IFD::<LittleEndian>(&mut reader, ifd_offset) {
                Ok(ifd) => ifd,
                Err(e) => return cf::Response::error(format!("Failed to read IFD: {}", e), 503),
            }
        }
        TIFFByteOrder::BigEndian => {
            tiff_reader.read_magic::<BigEndian>(&mut reader)?;
            let ifd_offset = tiff_reader.read_ifd_offset::<BigEndian>(&mut reader)?;
            match tiff_reader.read_IFD::<BigEndian>(&mut reader, ifd_offset) {
                Ok(ifd) => ifd,
                Err(e) => return cf::Response::error(format!("Failed to read IFD: {}", e), 503),
            }
        }
        _ => return cf::Response::error("Invalid byte order", 503),
    };

    // Read TIFF header
    // let tiff_header = cog::tiff_header::TiffHeader::new(&cog_header);

    // Generate lat/lng bounds
    let bounds = bounds::Bounds::from(&tile);
    // WEB Optimized COG: IFDs match zoom levels (0-8)
    
    /**
     * Now that we have the bounds...
     * - We could assume that the cog is in web mercator (3857) projection.
     * - tanslate bounds from latlng to internal COG projection. 
     *   (refer to https://github.com/georust/proj)
     * - bounds of the COG will be in metadata. the origin and resolution of each IFD 
     *   will be available. Review https://github.com/geospatial-jeff/aiocogeo/blob/master/aiocogeo/cog.py#L311-L318 for logic
     * - select the first IFD that covers the bounds and has a resolution that is higher 
     *   than or equal to the requested resolution. (review aiocogeo for this)
     * - fetch the blocks data for the selected IFD
     * - trim the data to only match the bounds
     * - reproject to tile projection (3857)
     * - SKIP FOR NOW: band selection???
     * - convert PNG.
     * - return PNG.
     */
    
    // After parsing header, I should be able to construct range requests to the IFDs of
    // choosing based on bounding box.

    // After fetch IFDs, trim to bounding box (how??), convert data (bytes, floats, etc, assume
    // uint8 for now), convert to PNG (review `image` crate), return PNG.

    // Assumptions:
    // - 512x512 tiles
    cf::ResponseBuilder::new()
        .with_status(200)
        .with_header("x-debug-src", &cog.src)?
        .with_header("x-debug-bounds", &format!("{:?}", bounds))?
        .with_header("x-debug-tile", &format!("{:?}", tile))?
        .ok(format!("{:?}", ifd))
}
