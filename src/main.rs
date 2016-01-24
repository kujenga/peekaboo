extern crate iron;
extern crate router;
extern crate redis;
extern crate image;
extern crate num;

use std::io;
use iron::prelude::*;
use iron::mime::Mime;
use iron::status;
use iron::modifier::Modifier;
use iron::response;
use router::Router;
use redis::Commands;
use image::{
    ImageBuffer,
};
use num::complex::Complex;

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

struct GenImg {
    img: ImageBuffer<image::Luma<u8>, Vec<u8>>
}

impl Modifier<Response> for GenImg {
    fn modify(self, res: &mut Response) {
        res.body = Some(Box::new(self));
    }
}

impl response::WriteBody for GenImg {
    fn write_body(&mut self, res: &mut response::ResponseBody) -> io::Result<()> {
        image::ImageLuma8(self.img.clone()).save(res, image::PNG)
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))
    }
}

fn main() {
	fn handler(_: &mut Request) -> IronResult<Response> {
        let content = "<a href=\"https://github.com/kujenga/peekaboo\">Peekaboo</a> server";
        let content_type = "text/html".parse::<Mime>().unwrap();
		Ok(Response::with((content_type, status::Ok, content)))
	}

    fn peek_handler(r: &mut Request) -> IronResult<Response> {
        let id = r.extensions.get::<Router>().unwrap().find("id").unwrap();
        let count = fetch_an_integer(id, true).unwrap_or(0i64);

        let mut img = ImageBuffer::new(512, 512);
        apply_julia(&mut img, count);

        let content_type = "image/png".parse::<Mime>().unwrap();
        Ok(Response::with((content_type, status::Ok, GenImg{img: img})))
    }

    fn peek_info_handler(r: &mut Request) -> IronResult<Response> {
        let id = r.extensions.get::<Router>().unwrap().find("id").unwrap();
        let val = fetch_an_integer(id, false).unwrap_or(0i64);

        Ok(Response::with((status::Ok, format!("'{}' has had {} visitors!", id, val))))
    }

    let mut router = Router::new();
    router.get("/", handler);
    router.get("/peek/:id", peek_handler);
    router.get("/peek/:id/info", peek_info_handler);

	Iron::new(router).http("localhost:2829").unwrap();
}

fn apply_julia(img: &mut ImageBuffer<image::Luma<u8>, Vec<u8>>, max_iters: i64) {

    // from: https://github.com/PistonDevelopers/image#62-generating-fractals

    let scalex = 4.0 / img.width() as f32;
    let scaley = 4.0 / img.height() as f32;

    for (x, y, pixel) in img.enumerate_pixels_mut() {
        // if x % 2 == 0 {
        //     *pixel = image::Luma([0u8])
        // } else {
        //     *pixel = image::Luma([255u8])
        // }

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
