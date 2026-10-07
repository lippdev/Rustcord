//! CDN reads never receive an Authorization header. Decode and thumbnail off the UI thread.
use image::{ImageReader, Limits};
use std::io::Cursor;
#[derive(Clone)]
pub struct ImageData {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}
pub(crate) async fn load(http: &reqwest::Client, url: &str) -> Option<ImageData> {
    crate::rest::safe_media(&serde_json::Value::String(url.to_owned()))?;
    let mut response = http.get(url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    const LIMIT: usize = 8 * 1024 * 1024;
    if response.content_length().is_some_and(|n| n > LIMIT as u64) {
        return None;
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.ok()? {
        if bytes.len().saturating_add(chunk.len()) > LIMIT {
            return None;
        }
        bytes.extend_from_slice(&chunk);
    }
    decode(bytes)
}
fn decode(bytes: Vec<u8>) -> Option<ImageData> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(32 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode().ok()?.thumbnail(320, 240).into_rgba8();
    Some(ImageData {
        width: image.width() as usize,
        height: image.height() as usize,
        rgba: image.into_raw(),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_images_are_rejected_and_thumbnails_are_bounded() {
        assert!(decode(b"not an image".to_vec()).is_none());
        let source = image::DynamicImage::new_rgba8(1024, 512);
        let mut bytes = Cursor::new(Vec::new());
        source
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        let decoded = decode(bytes.into_inner()).unwrap();
        assert_eq!((decoded.width, decoded.height), (320, 160));
        assert_eq!(decoded.rgba.len(), 320 * 160 * 4);
    }
}
