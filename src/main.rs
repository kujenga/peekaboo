extern crate axum;
extern crate handlebars;
extern crate http;
extern crate image;
extern crate num;
extern crate redis;
extern crate serde;
extern crate time;
extern crate tracing;
extern crate urlencoded;

use axum::{
    body::Bytes,
    error_handling::HandleErrorLayer,
    extract::{Path, Query},
    http::{header, HeaderValue, StatusCode},
    response::{Html, IntoResponse},
    routing::get,
    BoxError, Router,
};
use handlebars::Handlebars;
use image::ImageBuffer;
use redis::Commands;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, RwLock},
    time::Duration,
};
use time::OffsetDateTime;
use tower::ServiceBuilder;
use tower_http::{
    trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer},
    LatencyUnit, ServiceBuilderExt,
};
use urlencoded::UrlEncodedQuery;

mod img;

fn fetch_an_integer(key: &str, inc: bool) -> redis::RedisResult<i64> {
    // connect to redis
    let client = redis::Client::open("redis://127.0.0.1/")?;
    let mut con = client.get_connection()?;
    if inc {
        let cur = con.get(key).unwrap_or(0i64);
        let _: () = con.set(key, cur + 1)?;
    }
    con.get(key)
}

// from: https://github.com/iron/iron/blob/master/examples/time.rs
// struct ResponseTime;

// impl typemap::Key for ResponseTime {
//     type Value = OffsetDateTime;
// }

// impl BeforeMiddleware for ResponseTime {
//     fn before(&self, req: &mut Request) -> IronResult<()> {
//         req.extensions
//             .insert::<ResponseTime>(OffsetDateTime::now_utc());
//         Ok(())
//     }
// }

// impl AfterMiddleware for ResponseTime {
//     fn after(&self, req: &mut Request, res: Response) -> IronResult<Response> {
//         let delta = OffsetDateTime::now_utc() - *req.extensions.get::<ResponseTime>().unwrap();
//         println!("Request took: {} ms", delta.subsec_milliseconds());
//         Ok(res)
//     }
// }

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

#[derive(Clone, Debug)]
struct State {
    db: Arc<RwLock<HashMap<String, Bytes>>>,
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
        Path(id): Path<String>,
        params: Query<HashMap<String, String>>,
    ) -> Result<img::ImgWriter, StatusCode> {
        {
            let _ = match fetch_an_integer(id.as_str(), true) {
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

    async fn peek_info_handler(Path(id): Path<String>) -> Result<Html<String>, StatusCode> {
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
            count: fetch_an_integer(id.as_str(), false).unwrap_or(0i64),
        };

        match handlebars.render("info", &data) {
            Ok(result) => Ok(Html(result)),
            Err(err) => {
                println!("error rendering index: {}", err);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }

    // Build our database for holding the key/value pairs
    let state = State {
        db: Arc::new(RwLock::new(HashMap::new())),
    };

    // Build our middleware stack
    // ref: https://github.com/tower-rs/tower-http/blob/master/examples/axum-key-value-store/src/main.rs
    let middleware = ServiceBuilder::new()
        // Add high level tracing/logging to all requests
        .layer(
            TraceLayer::new_for_http()
                .on_body_chunk(|chunk: &Bytes, latency: Duration, _: &tracing::Span| {
                    tracing::trace!(size_bytes = chunk.len(), latency = ?latency, "sending body chunk")
                })
                .make_span_with(DefaultMakeSpan::new().include_headers(true))
                .on_response(DefaultOnResponse::new().include_headers(true).latency_unit(LatencyUnit::Micros)),
        )
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

    // let mut peek_chain = Chain::new(peek_handler);
    // peek_chain.link_before(ResponseTime);
    // peek_chain.link_after(ResponseTime);

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
