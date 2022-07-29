extern crate handlebars;
extern crate image;
extern crate iron;
extern crate num;
extern crate redis;
extern crate router;
extern crate serde;
extern crate time;
extern crate urlencoded;

use handlebars::Handlebars;
use image::{DynamicImage, ImageBuffer, ImageOutputFormat};
use iron::mime::Mime;
use iron::modifier::Modifier;
use iron::prelude::*;
use iron::{response, status, typemap, AfterMiddleware, BeforeMiddleware};
use num::complex::Complex;
use redis::Commands;
use router::Router;
use serde_derive::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io;
use time::OffsetDateTime;
use urlencoded::UrlEncodedQuery;

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
struct ResponseTime;

impl typemap::Key for ResponseTime {
    type Value = OffsetDateTime;
}

impl BeforeMiddleware for ResponseTime {
    fn before(&self, req: &mut Request) -> IronResult<()> {
        req.extensions
            .insert::<ResponseTime>(OffsetDateTime::now_utc());
        Ok(())
    }
}

impl AfterMiddleware for ResponseTime {
    fn after(&self, req: &mut Request, res: Response) -> IronResult<Response> {
        let delta = OffsetDateTime::now_utc() - *req.extensions.get::<ResponseTime>().unwrap();
        println!("Request took: {} ms", delta.subsec_milliseconds());
        Ok(res)
    }
}

// ImgWriter writes a generated image out to the request
struct ImgWriter {
    img: ImageBuffer<image::Luma<u8>, Vec<u8>>,
}

impl Modifier<Response> for ImgWriter {
    fn modify(self, res: &mut Response) {
        res.body = Some(Box::new(self));
    }
}

impl response::WriteBody for ImgWriter {
    fn write_body(&mut self, res: &mut dyn io::Write) -> io::Result<()> {
        // Write to intermediary buffer because Seek is required.
        let mut bytes: Vec<u8> = Vec::new();
        DynamicImage::ImageLuma8(self.img.clone())
            .write_to(&mut io::Cursor::new(&mut bytes), ImageOutputFormat::Png)
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;
        res.write_all(bytes.as_slice())
    }
}

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

fn main() {
    fn handler(_: &mut Request) -> IronResult<Response> {
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

        let content_type = "text/html".parse::<Mime>().unwrap();
        match handlebars.render("index", &data) {
            Ok(result) => Ok(Response::with((content_type, status::Ok, result))),
            Err(err) => Ok(Response::with((
                content_type,
                status::InternalServerError,
                format!("error: {}", err),
            ))),
        }
    }

    fn peek_handler(r: &mut Request) -> IronResult<Response> {
        {
            let id = r.extensions.get::<Router>().unwrap().find("id").unwrap();
            let _ = match fetch_an_integer(id, true) {
                Ok(v) => v,
                Err(e) => {
                    println!("error connecting to redis: {}", e);
                    0i64
                }
            };
        }

        let mut img = ImageBuffer::new(512, 512);
        let content_type = "image/png".parse::<Mime>().unwrap();

        match r.get_ref::<UrlEncodedQuery>() {
            Ok(ref hashmap) => match hashmap.get("t").map(|t| t[0].as_ref()) {
                Some("mandelbrot") => apply_mandelbrot(&mut img, 500),
                Some("julia") => apply_julia(&mut img, 500),
                Some(_) | None => return Ok(Response::with((status::NotFound, "type not found"))),
            },
            Err(urlencoded::UrlDecodingError::EmptyQuery) => {
                // turn the image white
                let mut img_sm = ImageBuffer::new(1, 1);
                apply_color(&mut img_sm, 255);
                return Ok(Response::with((
                    content_type,
                    status::Ok,
                    ImgWriter { img: img_sm },
                )));
            }
            Err(ref e) => {
                return Ok(Response::with((
                    status::BadRequest,
                    format!("invalid query: {}", e),
                )))
            }
        }

        Ok(Response::with((
            content_type,
            status::Ok,
            ImgWriter { img: img },
        )))
    }

    fn peek_info_handler(r: &mut Request) -> IronResult<Response> {
        let mut handlebars = Handlebars::new();
        match handlebars.register_template_string("base", BASE.to_string()) {
            Ok(_) => {}
            Err(err) => println!("error parsing header: {:?}", err),
        }
        match handlebars.register_template_string("info", INFO.to_string()) {
            Ok(_) => {}
            Err(err) => println!("error parsing info: {:?}", err),
        }
        // handlebars.register_template_file("header", &Path::new("./src/header.hbs")).ok().unwrap();
        // handlebars.register_template_file("info", &Path::new("./src/info.hbs")).ok().unwrap();

        let id = r.extensions.get::<Router>().unwrap().find("id").unwrap();
        let data = Peek {
            name: id.to_string(),
            count: fetch_an_integer(id, false).unwrap_or(0i64),
        };

        let content_type = "text/html".parse::<Mime>().unwrap();
        match handlebars.render("info", &data) {
            Ok(result) => Ok(Response::with((content_type, status::Ok, result))),
            Err(err) => Ok(Response::with((
                content_type,
                status::InternalServerError,
                format!("error: {}", err),
            ))),
        }
    }

    let mut peek_chain = Chain::new(peek_handler);
    peek_chain.link_before(ResponseTime);
    peek_chain.link_after(ResponseTime);

    let mut router = Router::new();
    router.get("/", handler, "index");
    router.get("/peek/:id", peek_chain, "peek");
    router.get("/peek/:id/info", peek_info_handler, "peek_info");

    // s.run()
    Iron::new(router).http("localhost:2829").unwrap();
}

fn apply_color(img: &mut ImageBuffer<image::Luma<u8>, Vec<u8>>, value: u8) {
    for (_, _, pixel) in img.enumerate_pixels_mut() {
        *pixel = image::Luma([value]);
    }
}

fn apply_mandelbrot(img: &mut ImageBuffer<image::Luma<u8>, Vec<u8>>, max_iters: i64) {
    // from: https://en.wikipedia.org/wiki/Mandelbrot_set#Escape_time_algorithm

    let scalex = 3.5 / img.width() as f32;
    let scaley = 2.0 / img.height() as f32;

    for (x, y, pixel) in img.enumerate_pixels_mut() {
        let x0 = x as f32 * scalex - 2.5;
        let y0 = y as f32 * scaley - 1.0;

        let mut z = Complex::new(x0, y0);
        // let c = z.clone();

        let mut iters = 0;
        for _ in 0..max_iters {
            if z.norm() > 2.0 {
                break;
            }
            let xt = z.re * z.re - z.im * z.im + x0;
            z.im = 2.0 * z.re * z.im + y0;
            z.re = xt;

            // for some reason this is about 2x slower
            // z = z * z + c;

            iters += 1;
        }

        *pixel = image::Luma([iters as u8]);
    }
}

fn apply_julia(img: &mut ImageBuffer<image::Luma<u8>, Vec<u8>>, max_iters: i64) {
    // from: https://github.com/PistonDevelopers/image#62-generating-fractals

    let scalex = 4.0 / img.width() as f32;
    let scaley = 4.0 / img.height() as f32;

    for (x, y, pixel) in img.enumerate_pixels_mut() {
        let cy = y as f32 * scaley - 2.0;
        let cx = x as f32 * scalex - 2.0;

        let mut z = Complex::new(cx, cy);
        let c = Complex::new(-0.4, 0.6);

        let mut i = 0;

        for t in 0..max_iters {
            if z.norm() > 2.0 {
                break;
            }
            z = z * z + c;
            i = t;
        }

        // Create an 8bit pixel of type Luma and value i
        // and assign in to the pixel at position (x, y)
        *pixel = image::Luma([i as u8]);
    }
}
