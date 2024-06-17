use serde::{Deserialize, Serialize};
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

pub async fn get_tile(_: cf::Request, ctx: cf::RouteContext<()>) -> cf::Result<cf::Response> {
    let tile = match tile::Tile::from(
        ctx.param("z").unwrap_or(&String::from("")),
        ctx.param("x").unwrap_or(&String::from("")),
        ctx.param("y").unwrap_or(&String::from("")),
    ) {
        Ok(tile) => tile,
        Err(e) => return cf::Response::error(format!("Invalid tile path: {}", e), 400),
    };

    // do lots of other things with tile...
    cf::Response::from_json(&bounds::Bounds::from(tile))
}
