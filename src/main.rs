extern crate iron;
extern crate router;
extern crate redis;
extern crate image;

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
        let _ = fetch_an_integer(id, true).unwrap_or(0i64);

        let img = ImageBuffer::from_fn(512, 512, |x, _| {
            if x % 2 == 0 {
                image::Luma([0u8])
            } else {
                image::Luma([255u8])
            }
        });

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
