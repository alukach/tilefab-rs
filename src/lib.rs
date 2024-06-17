use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
    let cog_header = match cog.fetch_header().await {
        Ok(body) => body,
        Err(e) => return cf::Response::error(format!("Failed to fetch COG: {}", e), 500),
    };

    // Generate lat/lng bounds
    let bounds = bounds::Bounds::from(&tile);

    cf::ResponseBuilder::new()
        .with_status(200)
        .with_header("x-debug-src", &cog.src)?
        .with_header("x-debug-bounds", &format!("{:?}", bounds))?
        .with_header("x-debug-tile", &format!("{:?}", tile))?
        .ok(cog_header)
}
