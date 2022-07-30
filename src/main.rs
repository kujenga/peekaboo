extern crate axum;
extern crate handlebars;
extern crate http;
extern crate image;
extern crate redis;
extern crate serde;
extern crate tracing;

use axum::{
    error_handling::HandleErrorLayer,
    extract::{Extension, Path, Query},
    http::{header, HeaderValue, StatusCode},
    response::{Html, IntoResponse},
    routing::get,
    BoxError, Router,
};
use handlebars::Handlebars;
use image::ImageBuffer;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::{collections::HashMap, net::SocketAddr, time::Duration};
use tower::ServiceBuilder;
use tower_http::{trace::TraceLayer, ServiceBuilderExt};

mod counter;
mod img;

static BASE: &'static str = r#"
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="utf-8">
    <title>{{title}}</title>
    <link rel="stylesheet" href="https://maxcdn.bootstrapcdn.com/bootstrap/4.0.0-alpha.2/css/bootstrap.min.css" integrity="sha384-y3tfxAZXuh4HwSYylfB+J125MxIs6mR5FOHamPBG064zB+AFeWH94NdvaCBm8qnd" crossorigin="anonymous">
    <style>
    body {
      padding-top: 5rem;
    }
    .lander {
      padding: 3rem 1.5rem;
      text-align: center;
    }
    </style>
</head>
<body>
<div class="container">
    {{> page}}
</div>
</body>
</html>
"#;

static INDEX: &'static str = r#"
{{#*inline "page"}}
    <div class="lander">
        <h1><a href="https://github.com/kujenga/peekaboo">Peekaboo</a> server</h1>
        <p>I see you!<p>
    </div>
{{/inline}}
{{~> base title=Peekaboo~}}
"#;

static INFO: &'static str = r#"
{{#*inline "page"}}
    <div class="lander">
        <h1><a href="https://github.com/kujenga/peekaboo">Peekaboo</a> server</h1>
        <p><strong>{{name}}</strong> has had {{count}} visitors!</p>
    </div>
{{/inline}}
{{~> base title=Peekaboo~}}
"#;

#[derive(Serialize, Deserialize, Debug)]
struct Peek {
    name: String,
    count: i64,
}

async fn handle_errors(err: BoxError) -> impl IntoResponse {
    if err.is::<tower::timeout::error::Elapsed>() {
        (
            StatusCode::REQUEST_TIMEOUT,
            "Request took too long".to_string(),
        )
    } else {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Unhandled internal error: {}", err),
        )
    }
}

#[tokio::main]
async fn main() {
    async fn handler() -> Result<Html<String>, StatusCode> {
        let mut handlebars = Handlebars::new();
        match handlebars.register_template_string("base", BASE.to_string()) {
            Ok(_) => {}
            Err(err) => println!("error parsing header: {:?}", err),
        }
        match handlebars.register_template_string("index", INDEX.to_string()) {
            Ok(_) => {}
            Err(err) => println!("error parsing index: {:?}", err),
        }

        let data: BTreeMap<String, String> = BTreeMap::new();

        match handlebars.render("index", &data) {
            Ok(result) => Ok(Html(result)),
            Err(err) => {
                println!("error rendering index: {}", err);
                Err(StatusCode::BAD_REQUEST)
            }
        }
    }

    async fn peek_handler(
        Extension(state): Extension<counter::State>,
        Path(id): Path<String>,
        params: Query<HashMap<String, String>>,
    ) -> Result<img::ImgWriter, StatusCode> {
        {
            let _ = match state.inc(id) {
                Ok(v) => v,
                Err(e) => {
                    println!("error connecting to redis: {}", e);
                    0i64
                }
            };
        }

        let mut img = ImageBuffer::new(512, 512);
        // let content_type = "image/png".parse::<Mime>().unwrap();

        match params.get("t").map(|t| t.as_str()) {
            Some("mandelbrot") => img::apply_mandelbrot(&mut img, 500),
            Some("julia") => img::apply_julia(&mut img, 500),
            Some(_) => return Err(StatusCode::NOT_FOUND),
            None => {
                // turn the image white
                let mut img_sm = ImageBuffer::new(1, 1);
                img::apply_color(&mut img_sm, 255);
                return Ok(img::ImgWriter { img: img_sm });
            }
        };

        Ok(img::ImgWriter { img: img })
    }

    async fn peek_info_handler(
        Extension(state): Extension<counter::State>,
        Path(id): Path<String>,
    ) -> Result<Html<String>, StatusCode> {
        let mut handlebars = Handlebars::new();
        match handlebars.register_template_string("base", BASE.to_string()) {
            Ok(_) => {}
            Err(err) => println!("error parsing header: {:?}", err),
        }
        match handlebars.register_template_string("info", INFO.to_string()) {
            Ok(_) => {}
            Err(err) => println!("error parsing info: {:?}", err),
        }

        let data = Peek {
            name: id.clone(),
            count: state.get(id).unwrap_or(0i64),
        };

        match handlebars.render("info", &data) {
            Ok(result) => Ok(Html(result)),
            Err(err) => {
                println!("error rendering index: {}", err);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }

    // Setup tracing
    tracing_subscriber::fmt::init();

    let state = counter::State::new("redis://127.0.0.1/");

    // Build our middleware stack
    // ref: https://github.com/tower-rs/tower-http/blob/master/examples/axum-key-value-store/src/main.rs
    let middleware = ServiceBuilder::new()
        // Add high level tracing/logging to all requests
        .layer(TraceLayer::new_for_http())
        // Handle errors
        .layer(HandleErrorLayer::new(handle_errors))
        // Set a timeout
        .timeout(Duration::from_secs(10))
        // Share the state with each handler via a request extension
        .add_extension(state)
        // Compress responses
        .compression()
        // Set a `Content-Type` if there isn't one already.
        .insert_response_header_if_not_present(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/octet-stream"),
        );

    let app = Router::new()
        .route("/", get(handler))
        .route("/peek/:id", get(peek_handler))
        .route("/peek/:id/info", get(peek_info_handler))
        .layer(middleware.into_inner());

    // run our app with hyper
    // `axum::Server` is a re-export of `hyper::Server`
    let addr = SocketAddr::from(([127, 0, 0, 1], 2829));
    tracing::debug!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}
