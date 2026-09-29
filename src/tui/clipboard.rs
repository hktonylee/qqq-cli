use crate::images::{ImageInput, MAX_IMAGE_BYTES};
use anyhow::{Context, Result, ensure};
use std::{io::Cursor, path::PathBuf};

pub enum Paste {
    Text(String),
    Image(ImageInput),
}
pub fn read() -> Result<Paste> {
    let mut clipboard = arboard::Clipboard::new().context("Clipboard unavailable")?;
    if let Ok(image) = clipboard.get_image() {
        return Ok(Paste::Image(encode(image)?));
    }
    Ok(Paste::Text(
        clipboard
            .get_text()
            .context("Clipboard contains no readable image or text")?,
    ))
}
pub fn encode(input: arboard::ImageData<'_>) -> Result<ImageInput> {
    ensure!(
        input.bytes.len() <= MAX_IMAGE_BYTES * 4,
        "Clipboard image exceeds 80 MiB raw limit"
    );
    let width = u32::try_from(input.width).context("Clipboard image width is too large")?;
    let height = u32::try_from(input.height).context("Clipboard image height is too large")?;
    ensure!(
        width > 0 && height > 0,
        "Clipboard image has empty dimensions"
    );
    let rgba = image::RgbaImage::from_raw(width, height, input.bytes.into_owned())
        .context("Invalid clipboard RGBA image")?;
    let mut data = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(rgba)
        .write_to(&mut data, image::ImageFormat::Png)
        .context("Cannot encode clipboard image as PNG")?;
    let input = ImageInput {
        name: "clipboard.png".into(),
        data: data.into_inner(),
    };
    input.media_type()?;
    Ok(input)
}
pub fn path_image(text: &str) -> Result<Option<ImageInput>> {
    if text.chars().count() > 1000 || text.contains(['\n', '\r']) {
        return Ok(None);
    }
    let Some(parts) = shlex::split(text.trim()).filter(|parts| parts.len() == 1) else {
        return Ok(None);
    };
    let mut path = PathBuf::from(&parts[0]);
    if let Some(relative) = parts[0].strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            path = PathBuf::from(home).join(relative);
        }
    }
    if !path.is_file() {
        return Ok(None);
    }
    match ImageInput::read(&path) {
        Ok(input) => Ok(Some(input)),
        Err(error) if error.to_string().starts_with("Unsupported image signature") => Ok(None),
        Err(error) => Err(error),
    }
}
