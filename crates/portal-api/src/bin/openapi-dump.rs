//! Dump the OpenAPI spec as pretty-printed JSON to stdout.
//!
//! `ApiDoc::openapi()` is assembled entirely at compile time, so this needs
//! no database or running server. Intended for regenerating the frontend's
//! `types.ts`:
//!
//! ```sh
//! cargo run -p portal-api --bin openapi-dump > openapi.json
//! ```

use portal_api::openapi::ApiDoc;
use utoipa::OpenApi;

fn main() {
    match ApiDoc::openapi().to_pretty_json() {
        Ok(json) => println!("{json}"),
        Err(e) => {
            eprintln!("failed to serialize OpenAPI spec: {e}");
            std::process::exit(1);
        }
    }
}
