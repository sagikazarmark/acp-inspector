//! One selected attachment, preserving its original media bytes or UTF-8 text.
use acp_inspector_core::v1;
use base64::{Engine, engine::general_purpose::STANDARD};

pub const MAX_MEDIA_BYTES: usize = 5 * 1024 * 1024;

#[derive(Clone, PartialEq)]
pub struct PromptMedia {
    pub name: String,
    pub bytes: usize,
    pub mime: &'static str,
    data: String,
    text_uri: Option<String>,
}

impl PromptMedia {
    pub async fn from_file(file: dioxus::html::FileData) -> Result<Self, String> {
        let name = file.name();
        let path = file.path();
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
        let mut media = Self::from_bytes(name, &bytes)?;
        if media.is_text() && path.is_absolute() {
            media.text_uri = Some(
                url::Url::from_file_path(path)
                    .map_err(|_| "Could not represent the selected file as a URI.")?
                    .into(),
            );
        }
        Ok(media)
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
            return Self::text(name, bytes);
        };
        Ok(Self {
            name,
            bytes: bytes.len(),
            mime,
            data: STANDARD.encode(bytes),
            text_uri: None,
        })
    }
    pub fn preview(&self) -> String {
        format!("data:{};base64,{}", self.mime, self.data)
    }
    pub fn content(&self) -> v1::ContentBlock {
        if let Some(uri) = &self.text_uri {
            v1::ContentBlock::Resource(v1::EmbeddedResource::new(
                v1::EmbeddedResourceResource::TextResourceContents(
                    v1::TextResourceContents::new(self.data.clone(), uri.clone())
                        .mime_type(self.mime.to_owned()),
                ),
            ))
        } else if self.is_audio() {
            v1::ContentBlock::Audio(v1::AudioContent::new(self.data.clone(), self.mime))
        } else {
            v1::ContentBlock::Image(v1::ImageContent::new(self.data.clone(), self.mime))
        }
    }
    pub fn is_audio(&self) -> bool {
        self.mime.starts_with("audio/")
    }
    pub fn is_text(&self) -> bool {
        self.text_uri.is_some()
    }
    pub fn text_preview(&self) -> String {
        self.data.chars().take(2000).collect()
    }
    fn text(name: String, bytes: &[u8]) -> Result<Self, String> {
        let extension = std::path::Path::new(&name)
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let binary_name = matches!(
            extension.as_str(),
            "png"
                | "jpg"
                | "jpeg"
                | "gif"
                | "webp"
                | "wav"
                | "mp3"
                | "pdf"
                | "zip"
                | "gz"
                | "exe"
                | "wasm"
                | "mp4"
                | "ogg"
                | "flac"
                | "doc"
                | "docx"
        );
        let text = std::str::from_utf8(bytes).map_err(|_| "Choose a UTF-8 text file or supported image/audio (identified by its file signature).")?;
        let binary_signature = [
            b"ID3".as_slice(),
            b"RIFF",
            b"fLaC",
            b"OggS",
            b"!<arch>\n",
            b"%PDF-",
            b"PK\x03\x04",
            b"PK\x05\x06",
            b"GIF",
            b"\x89PNG",
            b"\x7fELF",
            b"\0asm",
            b"\x1f\x8b",
        ]
        .iter()
        .any(|signature| bytes.starts_with(signature));
        if binary_name
            || binary_signature
            || text
                .chars()
                .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
        {
            return Err("Choose a UTF-8 text file or supported image/audio (identified by its file signature).".into());
        }
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let mut uri = url::Url::parse("attachment:///text/").expect("attachment URI");
        uri.path_segments_mut()
            .expect("hierarchical URI")
            .push(
                &NEXT
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                    .to_string(),
            )
            .push(&name);
        Ok(Self {
            name,
            bytes: bytes.len(),
            mime: "text/plain",
            data: text.to_owned(),
            text_uri: Some(uri.into()),
        })
    }
    pub fn advertised(&self, image: bool, audio: bool, embedded: bool) -> bool {
        if self.is_text() {
            embedded
        } else if self.is_audio() {
            audio
        } else {
            image
        }
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
    #[tokio::test]
    async fn native_text_file_metadata_keeps_escaped_absolute_uri() {
        let media = PromptMedia::from_file(dioxus::html::FileData::new(
            dioxus::html::SerializedFileData {
                path: "/tmp/a #?.rs".into(),
                size: 4,
                last_modified: 0,
                content_type: None,
                contents: Some(b"text".to_vec().into()),
            },
        ))
        .await
        .unwrap();
        assert_eq!(media.text_uri.as_deref(), Some("file:///tmp/a%20%23%3F.rs"));
    }
    #[test]
    fn text_is_literal_utf8_with_distinct_escaped_uris_and_a_bounded_preview() {
        let bytes = "\u{feff}<script>é</script>\r\n\t".as_bytes();
        let first = PromptMedia::from_bytes("a #?.rs".into(), bytes).unwrap();
        let second = PromptMedia::from_bytes("a #?.rs".into(), bytes).unwrap();
        assert_eq!(first.bytes, bytes.len());
        assert!(first.advertised(false, false, true));
        assert!(!first.advertised(true, true, false));
        assert_ne!(first.text_uri, second.text_uri);
        let v1::ContentBlock::Resource(resource) = first.content() else {
            panic!("resource")
        };
        let v1::EmbeddedResourceResource::TextResourceContents(text) = resource.resource else {
            panic!("text")
        };
        assert_eq!(text.text.as_bytes(), bytes);
        assert_eq!(text.mime_type.as_deref(), Some("text/plain"));
        assert!(text.uri.ends_with("a%20%23%3F.rs"));
        assert_eq!(
            PromptMedia::from_bytes("large.txt".into(), "é".repeat(3000).as_bytes())
                .unwrap()
                .text_preview()
                .chars()
                .count(),
            2000
        );
        assert!(
            PromptMedia::from_bytes("empty.txt".into(), b"")
                .unwrap()
                .is_text()
        );
        for bytes in [
            b"a\0b".as_slice(),
            b"\xff",
            b"%PDF-1.7",
            b"PK\x03\x04",
            b"ID3",
            b"fLaC",
            b"!<arch>\n",
            b"RIFF",
        ] {
            assert!(PromptMedia::from_bytes("x.txt".into(), bytes).is_err());
        }
    }
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
            assert!(attachment.advertised(false, true, false));
            assert!(!attachment.advertised(true, false, true));
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
        assert!(image.advertised(true, false, false));
        assert!(!image.advertised(false, true, true));
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
