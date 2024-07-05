use http_range_client::BufferedHttpRangeClient;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use worker as cf;

pub mod bounds;
pub mod cog;
pub mod errors;
pub mod tile;

#[derive(Debug, Deserialize, Serialize)]
struct GenericResponse {
    message: String,
}

#[cf::event(fetch)]
async fn main(req: cf::Request, env: cf::Env, _ctx: cf::Context) -> cf::Result<cf::Response> {
    cf::Router::new()
        .get_async("/", hello)
        .get_async("/head", head)
        .get_async("/:z/:x/:y", get_tile)
        .run(req, env)
        .await
}

pub async fn hello(_: cf::Request, _ctx: cf::RouteContext<()>) -> cf::Result<cf::Response> {
    cf::Response::from_json(&GenericResponse {
        message: "Just another tile server.".to_string(),
    })
}

/**
 * Helper to retrieve COG header and IFDs
 */
pub async fn head(req: cf::Request, _ctx: cf::RouteContext<()>) -> cf::Result<cf::Response> {
    // Retrieve src query parameter
    let query_params: HashMap<String, String> = match req.url() {
        Ok(url) => url.query_pairs().into_owned().collect(),
        Err(_) => return cf::Response::error("Failed to parse URL", 400),
    };
    let Some(src) = query_params.get("src") else {
        return cf::Response::error("src query parameter is required", 400);
    };

    let mut client = BufferedHttpRangeClient::new(src);
    let cog = match cog::Cog::new(&mut client).await {
        Ok(cog) => cog,
        Err(e) => return cf::Response::error(format!("{}", e), 500),
    };

    let mut res = cf::Response::from_json(&cog)?;
    res.headers_mut().append("x-debug-src", src)?;
    Ok(res)
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

    // TODO: Mv away from buffered client in favor of std client
    let mut client = BufferedHttpRangeClient::new(src);
    // client.min_req_size(1024);
    let _cog: cog::Cog = match cog::Cog::new(&mut client).await {
        Ok(cog) => cog,
        Err(e) => return cf::Response::error(format!("{}", e), 500),
    };

    // Generate lat/lng bounds
    let bounds = bounds::Bounds::from(&tile);

    cf::ResponseBuilder::new()
        .with_status(200)
        .with_header("x-debug-src", src)?
        .with_header("x-debug-bounds", &format!("{:?}", bounds))?
        .with_header("x-debug-tile", &format!("{:?}", tile))?
        .ok("TODO: Implement tile generation")
}
