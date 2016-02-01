extern crate iron;
extern crate router;
extern crate urlencoded;
extern crate redis;
extern crate image;
extern crate num;
extern crate time;
extern crate rustc_serialize;
extern crate handlebars;

use std::io;
use rustc_serialize::json::{Json, ToJson};
use std::collections::BTreeMap;
use time::precise_time_ns;
use iron::prelude::*;
use iron::{
    BeforeMiddleware,
    AfterMiddleware,
    typemap,
    status,
    response,
};
use iron::mime::Mime;
use iron::modifier::Modifier;
use urlencoded::UrlEncodedQuery;
use router::Router;
use redis::Commands;
use image::ImageBuffer;
use num::complex::Complex;
use handlebars::Handlebars;

fn fetch_an_integer(key: &str, inc: bool) -> redis::RedisResult<i64> {
    // connect to redis
    let client = try!(redis::Client::open("redis://127.0.0.1/"));
    let con = try!(client.get_connection());
    if inc {
        let cur = con.get(key).unwrap_or(0i64);
        let _ : () = try!(con.set(key, cur+1));
    }
    con.get(key)
}

// from: https://github.com/iron/iron/blob/master/examples/time.rs
struct ResponseTime;

impl typemap::Key for ResponseTime { type Value = u64; }

impl BeforeMiddleware for ResponseTime {
    fn before(&self, req: &mut Request) -> IronResult<()> {
        req.extensions.insert::<ResponseTime>(precise_time_ns());
        Ok(())
    }
}

impl AfterMiddleware for ResponseTime {
    fn after(&self, req: &mut Request, res: Response) -> IronResult<Response> {
        let delta = precise_time_ns() - *req.extensions.get::<ResponseTime>().unwrap();
        println!("Request took: {} ms", (delta as f64) / 1000000.0);
        Ok(res)
    }
}

// ImgWriter writes a generated image out to the request
struct ImgWriter {
    img: ImageBuffer<image::Luma<u8>, Vec<u8>>
}

impl Modifier<Response> for ImgWriter {
    fn modify(self, res: &mut Response) {
        res.body = Some(Box::new(self));
    }
}

impl response::WriteBody for ImgWriter {
    fn write_body(&mut self, res: &mut response::ResponseBody) -> io::Result<()> {
        image::ImageLuma8(self.img.clone()).save(res, image::PNG)
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))
    }
}

static INDEX: &'static str = r#"
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="utf-8">
    <title>Peekaboo</title>
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
    <div class="lander">
        <h1><a href=\"https://github.com/kujenga/peekaboo\">Peekaboo</a> server</h1>
    </div>
</div>
</body>
</html>
"#;

static INFO: &'static str = r#"
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="utf-8">
    <title>Peekaboo</title>
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
    <div class="lander">
        <h1><a href=\"https://github.com/kujenga/peekaboo\">Peekaboo</a> server</h1>
        <p>{{name}} has had {{count}} visitors!</p>
    </div>
</div>
</body>
</html>
"#;

struct Peek {
  name: String,
  count: i64,
}

impl ToJson for Peek {
  fn to_json(&self) -> Json {
    let mut m: BTreeMap<String, Json> = BTreeMap::new();
    m.insert("name".to_string(), self.name.to_json());
    m.insert("count".to_string(), self.count.to_json());
    m.to_json()
  }
}

fn main() {

	fn handler(_: &mut Request) -> IronResult<Response> {
        let content_type = "text/html".parse::<Mime>().unwrap();
		Ok(Response::with((content_type, status::Ok, INDEX)))
	}

    fn peek_handler(r: &mut Request) -> IronResult<Response> {
        {
            let id = r.extensions.get::<Router>().unwrap().find("id").unwrap();
            let _ = fetch_an_integer(id, true).unwrap_or(0i64);
        }

        let mut img = ImageBuffer::new(512, 512);
        let content_type = "image/png".parse::<Mime>().unwrap();

        match r.get_ref::<UrlEncodedQuery>() {
            Ok(ref hashmap) => {
                match hashmap.get("t").map(|t| t[0].as_ref()) {
                    Some("mandelbrot") => apply_mandelbrot(&mut img, 500),
                    Some("julia") => apply_julia(&mut img, 500),
                    Some(_) | None => {
                        return Ok(Response::with((status::NotFound, "type not found")))
                    },
                }
            },
            Err(urlencoded::UrlDecodingError::EmptyQuery) => {
                // turn the image white
                let mut img_sm = ImageBuffer::new(1, 1);
                apply_color(&mut img_sm, 255);
                return Ok(Response::with((content_type, status::Ok, ImgWriter{img: img_sm})))
            },
            Err(ref e) => {
                return Ok(Response::with((status::BadRequest, format!("invalid query: {}", e))))
            }
        }

        Ok(Response::with((content_type, status::Ok, ImgWriter{img: img})))
    }

    fn peek_info_handler(r: &mut Request) -> IronResult<Response> {
        let mut handlebars = Handlebars::new();
        handlebars.register_template_string("info", INFO.to_string()).ok().unwrap();

        let id = r.extensions.get::<Router>().unwrap().find("id").unwrap();
        let data = Peek {
            name: id.to_string(),
            count: fetch_an_integer(id, false).unwrap_or(0i64),
        };

        let content_type = "text/html".parse::<Mime>().unwrap();
        let result = handlebars.render("info", &data);
        Ok(Response::with((content_type, status::Ok, result.unwrap())))
    }

    let mut peek_chain = Chain::new(peek_handler);
    peek_chain.link_before(ResponseTime);
    peek_chain.link_after(ResponseTime);

    let mut router = Router::new();
    router.get("/", handler);
    router.get("/peek/:id", peek_chain);
    router.get("/peek/:id/info", peek_info_handler);

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
                break
            }
            let xt = z.re * z.re - z.im * z.im + x0;
            z.im = 2.0*z.re*z.im + y0;
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
                break
            }
            z = z * z + c;
            i = t;
        }

        // Create an 8bit pixel of type Luma and value i
        // and assign in to the pixel at position (x, y)
        *pixel = image::Luma([i as u8]);
    }
}
