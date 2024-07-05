# A WASM-friendly COG tile server

> [!CAUTION]  
> This project serves a workbench for me as I learn Rust. Set expectations accordingly.

## Development

### Why not use `{X}`?

<ul>

<li><details>
<summary><a href="//crates.io/crates/async_http_range_reader/"><code>async_http_range_reader</code></a>: fails to compile to WASM </summary>

```rs
[INFO]: 🎯  Checking for the Wasm target...
[INFO]: 🌀  Compiling to Wasm...
   Compiling either v1.13.0
   Compiling reqwest-middleware v0.3.1
   Compiling itertools v0.12.1
error[E0599]: no method named `poll_ready` found for struct `Client` in the current scope
   --> /Users/alukach/.cargo/registry/src/index.crates.io-6f17d22bba15001f/reqwest-middleware-0.3.1/src/client.rs:284:24
    |
284 |             self.inner.poll_ready(cx).map_err(crate::Error::Reqwest)
    |                        ^^^^^^^^^^ method not found in `Client`

error[E0599]: no method named `poll_ready` found for reference `&Client` in the current scope
   --> /Users/alukach/.cargo/registry/src/index.crates.io-6f17d22bba15001f/reqwest-middleware-0.3.1/src/client.rs:306:27
    |
306 |             (&self.inner).poll_ready(cx).map_err(crate::Error::Reqwest)
    |                           ^^^^^^^^^^ method not found in `&Client`

error[E0599]: no method named `timeout` found for struct `reqwest::RequestBuilder` in the current scope
   --> /Users/alukach/.cargo/registry/src/index.crates.io-6f17d22bba15001f/reqwest-middleware-0.3.1/src/client.rs:427:31
    |
427 |             inner: self.inner.timeout(timeout),
    |                               ^^^^^^^ method not found in `RequestBuilder`

For more information about this error, try `rustc --explain E0599`.
error: could not compile `reqwest-middleware` (lib) due to 3 previous errors
warning: build failed, waiting for other jobs to finish...
Error: Compiling your crate to WebAssembly failed
Caused by: Compiling your crate to WebAssembly failed
Caused by: failed to execute `cargo build`: exited with exit status: 101
  full command: cd "/Users/alukach/Projects/personal/rust-playground/tilefab-rs" && "cargo" "build" "--lib" "--release" "--target" "wasm32-unknown-unknown"
Error: wasm-pack exited with status exit status: 1
```

</details></li>

<li>

[`geotiff`](https://crates.io/crates/geotiff): built around local files. no support for `async` (ie not usable in WASM)

</li>
</ul>
