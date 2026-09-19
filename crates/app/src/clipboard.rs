//! The DOM clipboard seam: metadata reserves rows before bounded bytes arrive.
use crate::prompt_media::{MAX_MEDIA_BYTES, PromptMedia};
use base64::{Engine, engine::general_purpose::STANDARD};

pub const BRIDGE: &str = include_str!("clipboard.js");

#[derive(serde::Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ClipboardInput {
    Reserve {
        names: Vec<String>,
    },
    Finish {
        id: u64,
        data: Option<String>,
        error: Option<String>,
    },
}

pub fn image(
    name: String,
    data: Option<String>,
    error: Option<String>,
) -> Result<PromptMedia, String> {
    if let Some(error) = error {
        return Err(error);
    }
    let data = data.ok_or("Could not read clipboard image.")?;
    if data.len() > MAX_MEDIA_BYTES.div_ceil(3) * 4 {
        return Err("A media file may be at most 5 MiB.".into());
    }
    let bytes = STANDARD
        .decode(data)
        .map_err(|_| "Could not decode clipboard image.")?;
    let media = PromptMedia::from_bytes(name, &bytes)?;
    if media.is_audio() || media.is_text() {
        return Err("Paste a PNG, JPEG, GIF or WebP image.".into());
    }
    Ok(media)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pasted_bytes_use_signature_and_never_become_audio_or_paths() {
        let media = image("wrong.jpg".into(), Some(STANDARD.encode(b"GIF89a")), None).unwrap();
        assert_eq!(media.mime, "image/gif");
        assert!(
            image(
                "x.png".into(),
                Some(STANDARD.encode(b"RIFF\x04\0\0\0WAVE")),
                None
            )
            .is_err()
        );
        assert!(image("/tmp/picture.png".into(), None, None).is_err());
        assert!(
            image(
                "x".into(),
                Some("a".repeat(MAX_MEDIA_BYTES.div_ceil(3) * 4 + 1)),
                None
            )
            .is_err()
        );
        assert!(image("x".into(), None, Some("read failed".into())).is_err());
    }
}
