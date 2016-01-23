extern crate iron;
extern crate router;
extern crate redis;

use iron::prelude::*;
use iron::mime::Mime;
use iron::status;
use router::Router;
use redis::Commands;

fn fetch_an_integer(key: &str, inc: bool) -> redis::RedisResult<isize> {
    // connect to redis
    let client = try!(redis::Client::open("redis://127.0.0.1/"));
    let con = try!(client.get_connection());

    if inc {
        let cur: i64 = con.get(key).unwrap();
        let _ : () = try!(con.set(key, cur+1));
    }
    con.get(key)
}

fn main() {
	fn handler(_: &mut Request) -> IronResult<Response> {
        let content = "<a href=\"https://github.com/kujenga/peekaboo\">Peekaboo</a> server";
        let content_type = "text/html".parse::<Mime>().unwrap();
		Ok(Response::with((content_type, status::Ok, content)))
	}

    fn peek_handler(r: &mut Request) -> IronResult<Response> {
        let id = r.extensions.get::<Router>().unwrap().find("id").unwrap();
        let _ = fetch_an_integer(id, true).unwrap();

        Ok(Response::with((status::Ok, format!("I see you {}!", id))))
    }

    fn peek_info_handler(r: &mut Request) -> IronResult<Response> {
        let id = r.extensions.get::<Router>().unwrap().find("id").unwrap();
        let val = fetch_an_integer(id, false).unwrap();

        Ok(Response::with((status::Ok, format!("'{}' has had {} visitors!", id, val))))
    }

    let mut router = Router::new();
    router.get("/", handler);
    router.get("/peek/:id", peek_handler);
    router.get("/peek/:id/info", peek_info_handler);

	Iron::new(router).http("localhost:2829").unwrap();
}
