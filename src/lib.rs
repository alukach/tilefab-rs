use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use worker as cf;
pub mod bounds;
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
    let tile = match tile::Tile::from(
        ctx.param("z").unwrap_or(&String::from("")),
        ctx.param("x").unwrap_or(&String::from("")),
        ctx.param("y").unwrap_or(&String::from("")),
    ) {
        Ok(tile) => tile,
        Err(e) => return cf::Response::error(format!("Invalid tile path: {}", e), 400),
    };

    // Retrieve src query parameter
    let q: HashMap<_, _> = req.url().unwrap().query_pairs().into_owned().collect();
    let src = match q.get("src") {
        Some(src) => src,
        None => return cf::Response::error("src query parameter is required", 400),
    };

    // Generate lat/lng bounds
    let bounds = bounds::Bounds::from(&tile);

    cf::Response::ok(format!("{:?}\n{:?}\nSource {}", tile, bounds, src))
}
