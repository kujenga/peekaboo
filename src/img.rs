extern crate image;
extern crate num;

use axum::{
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use image::{DynamicImage, ImageBuffer, ImageOutputFormat};
use num::complex::Complex;
use std::io;

// ImgWriter writes a generated image out to the request
pub struct ImgWriter {
    pub img: ImageBuffer<image::Luma<u8>, Vec<u8>>,
}

impl IntoResponse for ImgWriter {
    // fn write_body(&mut self, res: &mut dyn io::Write) -> io::Result<()> {
    fn into_response(self) -> Response {
        // Write to intermediary buffer because Seek is required.
        let mut bytes: Vec<u8> = Vec::new();
        DynamicImage::ImageLuma8(self.img.clone())
            .write_to(&mut io::Cursor::new(&mut bytes), ImageOutputFormat::Png)
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))
            .unwrap();

        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/png"));

        (StatusCode::OK, headers, bytes).into_response()
    }
}

pub fn apply_color(img: &mut ImageBuffer<image::Luma<u8>, Vec<u8>>, value: u8) {
    for (_, _, pixel) in img.enumerate_pixels_mut() {
        *pixel = image::Luma([value]);
    }
}

pub fn apply_mandelbrot(img: &mut ImageBuffer<image::Luma<u8>, Vec<u8>>, max_iters: i64) {
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

pub fn apply_julia(img: &mut ImageBuffer<image::Luma<u8>, Vec<u8>>, max_iters: i64) {
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
