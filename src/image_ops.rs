use image::{
    codecs::{jpeg::JpegEncoder, png::PngEncoder, webp::WebPEncoder},
    imageops::FilterType,
    GenericImageView, ImageEncoder,
};
use std::io::Cursor;

use crate::error::ProxyError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetFormat {
    WebP,
    Jpeg,
    Png,
}

impl TargetFormat {
    pub fn content_type(&self) -> &'static str {
        match self {
            TargetFormat::WebP => "image/webp",
            TargetFormat::Jpeg => "image/jpeg",
            TargetFormat::Png => "image/png",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "webp" => Some(TargetFormat::WebP),
            "jpeg" | "jpg" => Some(TargetFormat::Jpeg),
            "png" => Some(TargetFormat::Png),
            _ => None,
        }
    }
}

pub fn is_animated_gif(bytes: &[u8]) -> bool {
    if bytes.len() < 6 || &bytes[0..3] != b"GIF" {
        return false;
    }
    let mut decoder = gif::DecodeOptions::new();
    decoder.set_color_output(gif::ColorOutput::Indexed);
    if let Ok(mut reader) = decoder.read_info(&mut &bytes[..]) {
        let mut frame_count = 0;
        while let Ok(Some(_)) = reader.read_next_frame() {
            frame_count += 1;
            if frame_count > 1 {
                return true;
            }
        }
    }
    false
}

pub fn is_animated_webp(bytes: &[u8]) -> bool {
    if bytes.len() >= 24 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        if &bytes[12..16] == b"VP8X" {
            let flags = bytes[20];
            return (flags & 0x02) != 0;
        }
    }
    false
}

pub fn is_svg(bytes: &[u8], content_type: &str) -> bool {
    if content_type.contains("image/svg+xml") {
        return true;
    }
    let slice = if bytes.len() > 100 { &bytes[..100] } else { bytes };
    let text = String::from_utf8_lossy(slice).to_lowercase();
    text.contains("<svg") || (text.contains("<?xml") && text.contains("svg"))
}

pub fn process_image(
    input_bytes: &[u8],
    target_format: TargetFormat,
    width: Option<u32>,
    height: Option<u32>,
    quality: Option<u8>,
) -> Result<(Vec<u8>, &'static str), ProxyError> {
    let img = image::load_from_memory(input_bytes)
        .map_err(|e| ProxyError::RemoteServer(format!("Failed to decode image: {e}")))?;

    let (orig_w, orig_h) = img.dimensions();

    let resized = match (width, height) {
        (Some(w), Some(h)) if w > 0 && h > 0 => img.resize(w, h, FilterType::Lanczos3),
        (Some(w), None) if w > 0 && orig_w > 0 => {
            let h = ((orig_h as u64 * w as u64) / orig_w as u64) as u32;
            img.resize_exact(w, h.max(1), FilterType::Lanczos3)
        }
        (None, Some(h)) if h > 0 && orig_h > 0 => {
            let w = ((orig_w as u64 * h as u64) / orig_h as u64) as u32;
            img.resize_exact(w.max(1), h, FilterType::Lanczos3)
        }
        _ => img,
    };

    let mut output_bytes = Vec::new();
    let q = quality.unwrap_or(80).clamp(1, 100);

    match target_format {
        TargetFormat::WebP => {
            // Encode WebP using image::codecs::webp
            let mut cursor = Cursor::new(&mut output_bytes);
            let rgb_img = resized.to_rgba8();
            let (w, h) = rgb_img.dimensions();
            let encoder = WebPEncoder::new_lossless(&mut cursor);
            encoder
                .write_image(&rgb_img, w, h, image::ExtendedColorType::Rgba8)
                .map_err(|e| ProxyError::Internal(format!("WebP encoding failed: {e}")))?;
        }
        TargetFormat::Jpeg => {
            let mut cursor = Cursor::new(&mut output_bytes);
            let rgb_img = resized.to_rgb8();
            let (w, h) = rgb_img.dimensions();
            let encoder = JpegEncoder::new_with_quality(&mut cursor, q);
            encoder
                .write_image(&rgb_img, w, h, image::ExtendedColorType::Rgb8)
                .map_err(|e| ProxyError::Internal(format!("JPEG encoding failed: {e}")))?;
        }
        TargetFormat::Png => {
            let mut cursor = Cursor::new(&mut output_bytes);
            let rgba_img = resized.to_rgba8();
            let (w, h) = rgba_img.dimensions();
            let encoder = PngEncoder::new(&mut cursor);
            encoder
                .write_image(&rgba_img, w, h, image::ExtendedColorType::Rgba8)
                .map_err(|e| ProxyError::Internal(format!("PNG encoding failed: {e}")))?;
        }
    }

    Ok((output_bytes, target_format.content_type()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_target_format_parsing() {
        assert_eq!(TargetFormat::from_str("webp"), Some(TargetFormat::WebP));
        assert_eq!(TargetFormat::from_str("WEBP"), Some(TargetFormat::WebP));
        assert_eq!(TargetFormat::from_str("jpeg"), Some(TargetFormat::Jpeg));
        assert_eq!(TargetFormat::from_str("jpg"), Some(TargetFormat::Jpeg));
        assert_eq!(TargetFormat::from_str("png"), Some(TargetFormat::Png));
        assert_eq!(TargetFormat::from_str("invalid"), None);
    }

    #[test]
    fn test_svg_detection() {
        assert!(is_svg(b"<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>", "image/svg+xml"));
        assert!(is_svg(b"<?xml version=\"1.0\"?><svg></svg>", "text/plain"));
        assert!(!is_svg(b"\x89PNG\r\n\x1a\n", "image/png"));
    }

    #[test]
    fn test_animated_webp_detection() {
        // Static WebP header (VP8 chunk)
        let static_webp = b"RIFF\x20\x00\x00\x00WEBPVP8 \x14\x00\x00\x00";
        assert!(!is_animated_webp(static_webp));

        // Extended WebP with animation flag bit 1 set (0x02)
        let mut anim_webp = vec![0u8; 30];
        anim_webp[0..4].copy_from_slice(b"RIFF");
        anim_webp[8..12].copy_from_slice(b"WEBP");
        anim_webp[12..16].copy_from_slice(b"VP8X");
        anim_webp[20] = 0x02; // animation flag
        assert!(is_animated_webp(&anim_webp));
    }

    #[test]
    fn test_process_image_resizing_and_encoding() {
        // Generate a 100x100 test image in PNG format
        let img = image::RgbImage::new(100, 100);
        let mut png_bytes = Vec::new();
        img.write_to(&mut Cursor::new(&mut png_bytes), image::ImageFormat::Png).unwrap();

        // 1. Convert to WebP and resize to width 50
        let (webp_out, mime) = process_image(&png_bytes, TargetFormat::WebP, Some(50), None, Some(80)).unwrap();
        assert_eq!(mime, "image/webp");
        assert!(!webp_out.is_empty());

        // Decode back and check dimensions
        let decoded = image::load_from_memory(&webp_out).unwrap();
        assert_eq!(decoded.dimensions(), (50, 50));

        // 2. Convert to JPEG with quality 90
        let (jpeg_out, mime) = process_image(&png_bytes, TargetFormat::Jpeg, Some(25), Some(25), Some(90)).unwrap();
        assert_eq!(mime, "image/jpeg");
        let decoded = image::load_from_memory(&jpeg_out).unwrap();
        assert_eq!(decoded.dimensions(), (25, 25));

        // 3. Convert to PNG
        let (png_out, mime) = process_image(&png_bytes, TargetFormat::Png, None, None, None).unwrap();
        assert_eq!(mime, "image/png");
        let decoded = image::load_from_memory(&png_out).unwrap();
        assert_eq!(decoded.dimensions(), (100, 100));
    }
}
