use std::process::{Command, Stdio};
use std::io::Write;
use std::thread;
use std::time::Duration;

use v4l::prelude::*;
use v4l::FourCC;
use v4l::video::Capture;
use v4l::buffer::Type;
use v4l::io::traits::CaptureStream;

use image::{Rgb, DynamicImage};
use rusttype::{Font, Scale};

use anyhow::Result;

fn main() -> Result<()> {
    // Open video device
    let device_path = "/dev/video0";
    let dev = v4l::Device::with_path(device_path)?;

    // Set format
    let width = 1280;
    let height = 720;
    let format = v4l::Format::new(width, height, FourCC::new(b"MJPG"));
    dev.set_format(&format)?;

    // Memory-mapped capture stream
    let mut stream = v4l::prelude::MmapStream::new(&dev, Type::VideoCapture)?;

    // Load font
    let font_data = include_bytes!("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf");
    let font = Font::try_from_bytes(font_data as &[u8]).unwrap();
    let scale = Scale { x: 20.0, y: 20.0 };

    // Spawn ffmpeg subprocess for raw RGB input
    let mut ffmpeg = Command::new("ffmpeg")
        .args([
            "-y",                       // overwrite output
            "-f", "rawvideo",           // raw video input
            "-pix_fmt", "rgb24",        // pixel format
            "-s", &format!("{}x{}", width, height), // resolution
            "-r", "30",                 // input FPS (match your camera)
            "-i", "-",                  // input from stdin
            "-c:v", "libx264",          // encode H.264
            "-pix_fmt", "yuv420p",      // output pixel format
            "output.mp4",
        ])
        .stdin(Stdio::piped())
        .spawn()?;

    let ffmpeg_stdin = ffmpeg.stdin.as_mut().unwrap();

    loop {
        // Capture frame
        let (buf, _) = stream.next()?;
        let mut img = image::load_from_memory_with_format(&buf, image::ImageFormat::Jpeg)?
            .to_rgb8();

        // Draw text overlay
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
                            let pixel = img.get_pixel_mut(px, py);
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

        // Write raw RGB24 frame to ffmpeg stdin
        ffmpeg_stdin.write_all(&img.as_raw())?;
    }
}

