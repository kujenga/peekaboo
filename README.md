# Peekaboo

[![Build Status](https://github.com/kujenga/peekaboo/actions/workflows/rust.yml/badge.svg)](https://github.com/kujenga/peekaboo/actions)

A simple, image-based tracker and hit counter.

This project is intended to help me learn and experiment with
[rust](https://www.rust-lang.org) in the context of a web server.

[Peekaboo](https://github.com/kujenga/peekaboo) is intended to be a simpler
version of the image-based trackers that are often used in emails in a
professional setting. Rather than signing up for some data hungry tracking
service or sending mail through a CRM tool, this can be applied more
selectively. I had the idea for this project after dealing with a somewhat
unresponsive landlord who owed me a security deposit.

The tracking handler is `/peek/:id`, where `:id` is an arbitrary string. Query
parameters determine what image is served back. The default will be a single
pixel, but representations of the
[mandelbrot](https://en.wikipedia.org/wiki/Mandelbrot_set) and
[julia](https://en.wikipedia.org/wiki/Julia_set) sets are also available just
for fun. For each request, a redis-backed counter is incremented. The value if
this counter is visible at `/peek/:id/info`. No authentication measures are
implemented at present, so your best bet is to choose a randomly generated
`:id`.

It uses [axum](https://github.com/tokio-rs/axum) for HTTP interactions, and
the [image](https://github.com/image-rs/image) crate to generate PNGs.

## Usage

To run the server for local development, you can use the following command:

```
RUST_LOG=tower_http=trace cargo run
```

To run Redis for storing counters persistently, run:

```
docker run --rm -it -p 6379:6379 redis:7
```

## Future ideas

- [ ] Support for image size specified in query parameters
- [ ] Keep a log of timestamps for each tracker hit
