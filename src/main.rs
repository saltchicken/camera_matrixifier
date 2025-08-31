use std::process::{Command, Stdio};
use std::io::Write;
use v4l::prelude::*;
use v4l::FourCC;
use v4l::video::Capture;
use v4l::io::traits::CaptureStream;
use v4l::buffer::Type;
use image::{Rgb, RgbImage, ImageBuffer}; 
use rusttype::{Font, Scale};
use anyhow::Result;

// ASCII conversion settings
const RESIZED_WIDTH: u32 = 80; // 160
const RESIZED_HEIGHT: u32 = 45; // 90
const ASCII_CHARS: &[char] = &[' ', '.', '\'', ',', ':', ';', 'c', 'l', 'x', 'o', 'k', 'X', 'd', 'O', '0', 'K', 'N'];

// Reduced output resolution for better performance
const OUTPUT_WIDTH: u32 = 1280;
const OUTPUT_HEIGHT: u32 = 720;

fn convert_pixel_to_ascii(intensity: u8) -> char {
    let index = (intensity as usize * (ASCII_CHARS.len() - 1)) / 255;
    ASCII_CHARS[index]
}

fn convert_to_ascii(gray_image: &image::GrayImage) -> Vec<Vec<char>> {
    let mut ascii_art = Vec::new();
    
    for y in 0..gray_image.height() {
        let mut row = Vec::new();
        for x in 0..gray_image.width() {
            let pixel = gray_image.get_pixel(x, y);
            let intensity = pixel[0];
            row.push(convert_pixel_to_ascii(intensity));
        }
        ascii_art.push(row);
    }
    
    ascii_art
}

fn create_ascii_image(ascii_art: &[Vec<char>], font: &Font, scale: Scale, output_width: u32, output_height: u32) -> RgbImage {
    let mut img = ImageBuffer::new(output_width, output_height);
    
    // Fill with black background
    for pixel in img.pixels_mut() {
        *pixel = Rgb([0, 0, 0]);
    }
    
    let v_metrics = font.v_metrics(scale);
    let char_height = v_metrics.ascent - v_metrics.descent;
    let char_width = scale.x * 1.0; // Approximate character width for monospace
    
    for (row_idx, row) in ascii_art.iter().enumerate() {
        for (col_idx, &ch) in row.iter().enumerate() {
            let x_pos = col_idx as f32 * char_width;
            let y_pos = row_idx as f32 * char_height + v_metrics.ascent;
            
            let offset = rusttype::point(x_pos, y_pos);
            
            let glyph = font.glyph(ch).scaled(scale).positioned(offset);
            if let Some(bb) = glyph.pixel_bounding_box() {
                glyph.draw(|x, y, v| {
                    let px = (bb.min.x + x as i32) as u32;
                    let py = (bb.min.y + y as i32) as u32;
                    
                    if px < output_width && py < output_height {
                        let pixel = img.get_pixel_mut(px, py);
                        // Green text on black background
                        *pixel = Rgb([
                            0,
                            (v * 255.0) as u8,
                            0,
                        ]);
                    }
                });
            }
        }
    }
    
    img
}

fn apply_blue_mask(img: &mut RgbImage) {
    // Convert BGR ranges to RGB (OpenCV uses BGR, image crate uses RGB)
    let lower_blue = [0, 0, 100];   // [100, 0, 0] BGR -> [0, 0, 100] RGB
    let upper_blue = [120, 100, 255]; // [255, 100, 120] BGR -> [120, 100, 255] RGB
    
    for pixel in img.pixels_mut() {
        let [r, g, b] = pixel.0;
        
        // Check if pixel is in blue range
        if r >= lower_blue[0] && r <= upper_blue[0] &&
           g >= lower_blue[1] && g <= upper_blue[1] &&
           b >= lower_blue[2] && b <= upper_blue[2] {
            *pixel = Rgb([0, 0, 0]); // Set to black
        }
    }
}

fn main() -> Result<()> {
    let device_path = "/dev/video0";
    let dev = v4l::Device::with_path(device_path)?;
    
    // Set the format
    let format = v4l::Format::new(320, 180, FourCC::new(b"MJPG"));
    dev.set_format(&format)?;
    
    // Memory-mapped capture stream
    let mut stream = MmapStream::new(&dev, Type::VideoCapture)?;
    
    // Load a font
    let font_data = include_bytes!("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf");
    let font = Font::try_from_bytes(font_data as &[u8]).unwrap();
    let scale = Scale { x: 16.0, y: 16.0 }; // Smaller font for ASCII art
    
    println!("Starting ffmpeg process...");
    
    // Start ffmpeg process with stdin pipe
    let mut ffmpeg = Command::new("ffmpeg")
        .arg("-y") // Overwrite output file
        .arg("-f").arg("rawvideo")  // Input format
        .arg("-pixel_format").arg("rgb24")  // RGB format
        .arg("-video_size").arg(format!("{}x{}", OUTPUT_WIDTH, OUTPUT_HEIGHT))
        .arg("-framerate").arg("10")
        .arg("-i").arg("pipe:0")  // Read from stdin
        .arg("-c:v").arg("libx264")
        .arg("-pix_fmt").arg("yuv420p")
        .arg("-preset").arg("ultrafast")  // Fast encoding preset
        .arg("ascii_output.mp4")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    
    let mut stdin = ffmpeg.stdin.take().unwrap();
    
    println!("Recording ASCII frames... Press Ctrl+C to stop");
    
    let mut frame_count = 0;
    let max_frames = 30 * 5; // 5 seconds at 30 FPS
    
    // Real-time capture and streaming
    loop {
        // Capture a frame
        let (buf, _) = stream.next()?;
        
        // Decode MJPG to RGB
        let mut img = image::load_from_memory_with_format(&buf, image::ImageFormat::Jpeg)?
            .to_rgb8();
        
        // Apply blue masking (similar to Python version)
        apply_blue_mask(&mut img);
        
        // Resize image to reduce resolution for ASCII conversion
        let resized_img = image::imageops::resize(
            &img, 
            RESIZED_WIDTH, 
            RESIZED_HEIGHT, 
            image::imageops::FilterType::Nearest
        );
        
        // Convert to grayscale
        let gray_img = image::imageops::grayscale(&resized_img);
        
        // Convert to ASCII
        let ascii_art = convert_to_ascii(&gray_img);
        
        // Create final image with ASCII text at reduced resolution
        let ascii_image = create_ascii_image(&ascii_art, &font, scale, OUTPUT_WIDTH, OUTPUT_HEIGHT);
        
        // Convert image to raw RGB bytes and write to ffmpeg stdin
        let raw_data: Vec<u8> = ascii_image.into_raw();
        if let Err(e) = stdin.write_all(&raw_data) {
            eprintln!("Error writing to ffmpeg: {}", e);
            break;
        }
        
        frame_count += 1;
        println!("Streamed frame {}/{}", frame_count, max_frames);
        
        // Stop after max_frames
        if frame_count >= max_frames {
            break;
        }
        
        // Small delay to control frame rate
        // thread::sleep(Duration::from_millis(33)); // ~30 FPS
    }
    
    // Close stdin to signal end of input to ffmpeg
    drop(stdin);
    
    println!("Waiting for ffmpeg to finish encoding...");
    
    // Wait for ffmpeg to complete
    let status = ffmpeg.wait()?;
    
    if status.success() {
        println!("Video saved as ascii_output.mp4");
    } else {
        println!("Error: ffmpeg process failed with status: {}", status);
    }
    
    Ok(())
}
