use byteorder::{BigEndian, ByteOrder, LittleEndian};
use byteorder_async::ReaderToByteOrder;
use http_range_client::{AsyncBufferedHttpRangeClient, AsyncHttpRangeClient};
use num::FromPrimitive;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use worker as cf;

#[macro_use]
extern crate enum_primitive;

pub mod bounds;
pub mod cog;
pub mod geotiff;
pub mod reader;
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

    // Read some data
    let mut reader = AsyncBufferedHttpRangeClient::new(src);
    reader.min_req_size(4096);

    // match TIFFByteOrder::from_u16(reader.byte_order().read_u16::<LittleEndian>()?) {
    //     Some(TIFFByteOrder::LittleEndian) => Ok(TIFFByteOrder::LittleEndian),
    //     Some(TIFFByteOrder::BigEndian) => Ok(TIFFByteOrder::BigEndian),
    //     None => Err(Error::new(
    //         ErrorKind::Other,
    //         format!("Invalid byte order in header."),
    //     )),
    // }

    // let bytes = match reader.get_bytes(15).await {
    //     Ok(bytes) => bytes,
    //     Err(e) => return cf::Response::error(format!("Failed to read COG: {}", e), 500),
    // };

    let bytes = match reader.get_range(0, 15).await {
        Ok(bytes) => bytes,
        Err(e) => return cf::Response::error(format!("Failed to read COG: {}", e), 500),
    };

    let cog = cog::COG {
        src: src.to_string(),
    };
    // Fetch COG header
    // let cog_header = match cog.fetch_header().await {
    //     Ok(body) => body,
    //     Err(e) => return cf::Response::error(format!("Failed to fetch COG: {}", e), 500),
    // };

    // Generate lat/lng bounds
    let bounds = bounds::Bounds::from(&tile);

    cf::ResponseBuilder::new()
        .with_status(200)
        .with_header("x-debug-src", &cog.src)?
        .with_header("x-debug-bounds", &format!("{:?}", bounds))?
        .with_header("x-debug-tile", &format!("{:?}", tile))?
        .ok(format!("Bytes: {:?}", bytes))
}
