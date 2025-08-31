use std::thread;
use std::time::Duration;
use v4l::prelude::*;
use v4l::FourCC;
use v4l::video::Capture;
use v4l::io::traits::CaptureStream;
use v4l::buffer::Type;
use image::{Rgb}; // Keep only what you use
use rusttype::{Font, Scale};
use anyhow::Result;

fn main() -> Result<()> {
    let device_path = "/dev/video0";
    let dev = v4l::Device::with_path(device_path)?;

    // Set the format
    let format = v4l::Format::new(1280, 720, FourCC::new(b"MJPG"));
    dev.set_format(&format)?;

    // Memory-mapped capture stream
    let mut stream = MmapStream::new(&dev, Type::VideoCapture)?;

    // Load a font
    let font_data = include_bytes!("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf");
    let font = Font::try_from_bytes(font_data as &[u8]).unwrap();
    let scale = Scale { x: 20.0, y: 20.0 };

    loop {
        // Capture a frame
        let (buf, _) = stream.next()?;

        // Decode MJPG to RGB
        let img = image::load_from_memory_with_format(&buf, image::ImageFormat::Jpeg)?
            .to_rgb8();

        let width = img.width();
        let height = img.height();
        let mut img_draw = img.clone();

        // Draw text
        let text = "Hello Rust!";
        for (i, c) in text.chars().enumerate() {
            let v_metrics = font.v_metrics(scale);
            let offset = rusttype::point(10.0 + i as f32 * 15.0, 30.0 + v_metrics.ascent);
            for glyph in font.layout(&c.to_string(), scale, offset) {
                if let Some(bb) = glyph.pixel_bounding_box() {
                    glyph.draw(|x, y, v| {
                        let px = (bb.min.x + x as i32) as u32;
                        let py = (bb.min.y + y as i32) as u32;
                        if px < width && py < height {
                            let pixel = img_draw.get_pixel_mut(px, py);
                            *pixel = Rgb([
                                (v * 255.0) as u8,
                                pixel[1],
                                pixel[2],
                            ]);
                        }
                    });
                }
            }
        }

        // Save a frame
        img_draw.save("frame.png")?;

        thread::sleep(Duration::from_millis(100));
    }
}
