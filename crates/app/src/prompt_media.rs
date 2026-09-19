//! One selected image or audio file, encoded without changing its bytes.
use acp_inspector_core::v1;
use base64::{Engine, engine::general_purpose::STANDARD};

pub const MAX_MEDIA_BYTES: usize = 5 * 1024 * 1024;

#[derive(Clone, PartialEq)]
pub struct PromptMedia {
    pub name: String,
    pub bytes: usize,
    pub mime: &'static str,
    data: String,
}

impl PromptMedia {
    pub async fn from_file(file: dioxus::html::FileData) -> Result<Self, String> {
        let name = file.name();
        #[cfg(not(target_arch = "wasm32"))]
        let bytes = if file
            .inner()
            .downcast_ref::<dioxus::html::SerializedFileData>()
            .is_none_or(|data| data.contents.is_none())
        {
            let path = file.path();
            tokio::task::spawn_blocking(move || -> Result<Vec<u8>, String> {
                use std::io::Read;
                if !std::fs::metadata(&path)
                    .map_err(|e| e.to_string())?
                    .is_file()
                {
                    return Err("Choose a regular media file.".into());
                }
                let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
                if !file.metadata().map_err(|e| e.to_string())?.is_file() {
                    return Err("Choose a regular media file.".into());
                }
                let mut bytes = Vec::new();
                file.take((MAX_MEDIA_BYTES + 1) as u64)
                    .read_to_end(&mut bytes)
                    .map_err(|e| e.to_string())?;
                Ok(bytes)
            })
            .await
            .map_err(|e| e.to_string())??
        } else if file
            .inner()
            .downcast_ref::<dioxus::html::SerializedFileData>()
            .is_some_and(|data| data.contents.is_some())
        {
            file.read_bytes().await.map_err(|e| e.to_string())?.to_vec()
        } else {
            return Err("Choose a readable regular media file.".into());
        };
        #[cfg(target_arch = "wasm32")]
        let bytes = {
            if file.size() > MAX_MEDIA_BYTES as u64 {
                return Err("A media file may be at most 5 MiB.".into());
            }
            file.read_bytes().await.map_err(|e| e.to_string())?.to_vec()
        };
        Self::from_bytes(name, &bytes)
    }
    pub fn from_bytes(name: String, bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_MEDIA_BYTES {
            return Err("A media file may be at most 5 MiB.".into());
        }
        let mime = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            "image/png"
        } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
            "image/jpeg"
        } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
            "image/gif"
        } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
            "image/webp"
        } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WAVE") {
            "audio/wav"
        } else if is_mp3(bytes) {
            "audio/mpeg"
        } else {
            return Err(
                "Choose PNG, JPEG, GIF, WebP, WAV or MP3 (identified by its file signature)."
                    .into(),
            );
        };
        Ok(Self {
            name,
            bytes: bytes.len(),
            mime,
            data: STANDARD.encode(bytes),
        })
    }
    pub fn preview(&self) -> String {
        format!("data:{};base64,{}", self.mime, self.data)
    }
    pub fn content(&self) -> v1::ContentBlock {
        if self.is_audio() {
            v1::ContentBlock::Audio(v1::AudioContent::new(self.data.clone(), self.mime))
        } else {
            v1::ContentBlock::Image(v1::ImageContent::new(self.data.clone(), self.mime))
        }
    }
    pub fn is_audio(&self) -> bool {
        self.mime.starts_with("audio/")
    }
    pub fn advertised(&self, image: bool, audio: bool) -> bool {
        if self.is_audio() { audio } else { image }
    }
}

fn is_mp3(mut bytes: &[u8]) -> bool {
    // ID3 is metadata, also used by AAC. Skip bounded ID3v2 tags before
    // identifying the actual MPEG Layer III header, including free-format.
    while bytes.starts_with(b"ID3") {
        if bytes.len() < 10
            || !(2..=4).contains(&bytes[3])
            || bytes[4] == 0xff
            || bytes[6..10].iter().any(|byte| byte & 0x80 != 0)
        {
            return false;
        }
        let size = bytes[6..10]
            .iter()
            .fold(0usize, |size, byte| (size << 7) | usize::from(*byte));
        let footer = if bytes[3] == 4 && bytes[5] & 0x10 != 0 {
            10
        } else {
            0
        };
        let Some(rest) = bytes.get(10 + size + footer..) else {
            return false;
        };
        bytes = rest;
    }
    bytes.len() >= 4
        && bytes[0] == 0xff
        && bytes[1] & 0xe0 == 0xe0
        && bytes[1] & 0x18 != 0x08
        && bytes[1] & 0x06 == 0x02
        && bytes[2] >> 4 != 15
        && bytes[2] & 0x0c != 0x0c
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wav_and_mp3_keep_their_bytes_and_use_audio_content() {
        for (bytes, mime) in [
            (b"RIFF\x04\0\0\0WAVE".as_slice(), "audio/wav"),
            (
                b"ID3\x04\0\0\0\0\0\0\xff\xfb\x90\x64".as_slice(),
                "audio/mpeg",
            ),
            (b"\xff\xfb\x90\x64".as_slice(), "audio/mpeg"),
            (b"\xff\xfb\x00\x64".as_slice(), "audio/mpeg"),
        ] {
            let attachment = PromptMedia::from_bytes("wrong.png".into(), bytes).unwrap();
            assert_eq!(attachment.mime, mime);
            assert!(attachment.advertised(false, true));
            assert!(!attachment.advertised(true, false));
            let v1::ContentBlock::Audio(audio) = attachment.content() else {
                panic!("audio block")
            };
            assert_eq!(STANDARD.decode(audio.data).unwrap(), bytes);
            assert_eq!(audio.mime_type, mime);
        }
        assert!(PromptMedia::from_bytes("wrong.mp3".into(), b"\xff\xff\xff\xff").is_err());
        assert!(PromptMedia::from_bytes("aac.mp3".into(), b"\xff\xf1\x50\x80").is_err());
        for bytes in [
            b"ID3\x04\0\0\0\0\0\0\xff\xf1\x50\x80".as_slice(),
            b"ID3\x04\0\0\0\0\0\0",
            b"ID3\x04\0\0\x7f\x7f\x7f\x7f\xff\xfb\x90\x64",
        ] {
            assert!(PromptMedia::from_bytes("tagged.mp3".into(), bytes).is_err());
        }
    }
    #[test]
    fn signature_controls_mime_and_encoded_content_preserves_every_byte() {
        let bytes = b"\x89PNG\r\n\x1a\nunchanged";
        let image = PromptMedia::from_bytes("misnamed.jpg".into(), bytes).unwrap();
        assert_eq!(image.mime, "image/png");
        assert!(image.advertised(true, false));
        assert!(!image.advertised(false, true));
        let v1::ContentBlock::Image(content) = image.content() else {
            panic!("image")
        };
        assert_eq!(STANDARD.decode(content.data).unwrap(), bytes);
        assert!(image.preview().starts_with("data:image/png;base64,"));
    }
    #[test]
    fn unsupported_or_oversized_selections_fail_without_an_attachment() {
        assert!(PromptMedia::from_bytes("x.png".into(), b"not an image").is_err());
        let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
        bytes.resize(MAX_MEDIA_BYTES + 1, 0);
        assert!(PromptMedia::from_bytes("x".into(), &bytes).is_err());
    }
}
