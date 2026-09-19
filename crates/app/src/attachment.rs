//! One selected attachment, preserving its original binary bytes or UTF-8 text.
use acp_inspector_core::v1;
use base64::{Engine, engine::general_purpose::STANDARD};

pub const MAX_ATTACHMENT_BYTES: usize = 5 * 1024 * 1024;

#[derive(Clone, PartialEq)]
pub struct Attachment {
    pub name: String,
    pub bytes: usize,
    content: AttachmentContent,
}

#[derive(Clone, PartialEq)]
enum AttachmentContent {
    Image { mime: &'static str, base64: String },
    Audio { mime: &'static str, base64: String },
    EmbeddedText { text: String, uri: String },
    Pdf { base64: String, uri: String },
}

/// Presentation data, without exposing or reclassifying the stored payload.
pub enum AttachmentPreview {
    Image { src: String },
    Audio { src: String },
    Text { excerpt: String },
    Pdf,
}

impl Attachment {
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
                file.take((MAX_ATTACHMENT_BYTES + 1) as u64)
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
            if file.size() > MAX_ATTACHMENT_BYTES as u64 {
                return Err("A media file may be at most 5 MiB.".into());
            }
            file.read_bytes().await.map_err(|e| e.to_string())?.to_vec()
        };
        let mut media = Self::from_bytes(name, &bytes)?;
        if let AttachmentContent::EmbeddedText { uri, .. } | AttachmentContent::Pdf { uri, .. } =
            &mut media.content
            && path.is_absolute()
        {
            *uri = url::Url::from_file_path(path)
                .map_err(|_| "Could not represent the selected file as a URI.")?
                .into();
        }
        Ok(media)
    }
    pub fn from_bytes(name: String, bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_ATTACHMENT_BYTES {
            return Err("A media file may be at most 5 MiB.".into());
        }
        let image = |mime| AttachmentContent::Image {
            mime,
            base64: STANDARD.encode(bytes),
        };
        let audio = |mime| AttachmentContent::Audio {
            mime,
            base64: STANDARD.encode(bytes),
        };
        let content = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            image("image/png")
        } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
            image("image/jpeg")
        } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
            image("image/gif")
        } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
            image("image/webp")
        } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WAVE") {
            audio("audio/wav")
        } else if is_mp3(bytes) {
            audio("audio/mpeg")
        } else if is_pdf(bytes) {
            AttachmentContent::Pdf {
                base64: STANDARD.encode(bytes),
                uri: attachment_uri("pdf", &name),
            }
        } else {
            return Self::text(name, bytes);
        };
        Ok(Self {
            name,
            bytes: bytes.len(),
            content,
        })
    }
    pub fn mime(&self) -> &'static str {
        match &self.content {
            AttachmentContent::Image { mime, .. } | AttachmentContent::Audio { mime, .. } => mime,
            AttachmentContent::EmbeddedText { .. } => "text/plain",
            AttachmentContent::Pdf { .. } => "application/pdf",
        }
    }
    pub fn preview(&self) -> AttachmentPreview {
        match &self.content {
            AttachmentContent::Image { mime, base64 } => AttachmentPreview::Image {
                src: format!("data:{mime};base64,{base64}"),
            },
            AttachmentContent::Audio { mime, base64 } => AttachmentPreview::Audio {
                src: format!("data:{mime};base64,{base64}"),
            },
            AttachmentContent::EmbeddedText { text, .. } => AttachmentPreview::Text {
                excerpt: text.chars().take(2000).collect(),
            },
            AttachmentContent::Pdf { .. } => AttachmentPreview::Pdf,
        }
    }
    pub fn content(&self) -> v1::ContentBlock {
        match &self.content {
            AttachmentContent::Pdf { base64, uri } => v1::ContentBlock::Resource(
                v1::EmbeddedResource::new(v1::EmbeddedResourceResource::BlobResourceContents(
                    v1::BlobResourceContents::new(base64.clone(), uri.clone())
                        .mime_type(self.mime().to_owned()),
                )),
            ),
            AttachmentContent::EmbeddedText { text, uri } => v1::ContentBlock::Resource(
                v1::EmbeddedResource::new(v1::EmbeddedResourceResource::TextResourceContents(
                    v1::TextResourceContents::new(text.clone(), uri.clone())
                        .mime_type(self.mime().to_owned()),
                )),
            ),
            AttachmentContent::Audio { mime, base64 } => {
                v1::ContentBlock::Audio(v1::AudioContent::new(base64.clone(), *mime))
            }
            AttachmentContent::Image { mime, base64 } => {
                v1::ContentBlock::Image(v1::ImageContent::new(base64.clone(), *mime))
            }
        }
    }
    pub fn is_image(&self) -> bool {
        matches!(self.content, AttachmentContent::Image { .. })
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
        let text = std::str::from_utf8(bytes).map_err(|_| "Choose a UTF-8 text file or supported image/audio/PDF (identified by its file signature).")?;
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
            return Err("Choose a UTF-8 text file or supported image/audio/PDF (identified by its file signature).".into());
        }
        let uri = attachment_uri("text", &name);
        Ok(Self {
            name,
            bytes: bytes.len(),
            content: AttachmentContent::EmbeddedText {
                text: text.to_owned(),
                uri,
            },
        })
    }
    pub fn require_advertisement(
        &self,
        image: bool,
        audio: bool,
        embedded: bool,
    ) -> Result<(), String> {
        let (advertised, kind) = match &self.content {
            AttachmentContent::Image { .. } => (image, "image"),
            AttachmentContent::Audio { .. } => (audio, "audio"),
            AttachmentContent::EmbeddedText { .. } | AttachmentContent::Pdf { .. } => {
                (embedded, "embedded context")
            }
        };
        if advertised {
            Ok(())
        } else {
            Err(format!("The Agent did not advertise {kind} prompts."))
        }
    }
}

fn attachment_uri(kind: &str, name: &str) -> String {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let mut uri = url::Url::parse("attachment:///").expect("attachment URI");
    uri.path_segments_mut()
        .expect("hierarchical URI")
        .pop_if_empty()
        .push(kind)
        .push(
            &NEXT
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                .to_string(),
        )
        .push(name);
    uri.into()
}

fn is_pdf(bytes: &[u8]) -> bool {
    // Header identification only, not a claim that the document is well-formed.
    matches!(
        bytes.get(..8),
        Some(
            b"%PDF-1.0"
                | b"%PDF-1.1"
                | b"%PDF-1.2"
                | b"%PDF-1.3"
                | b"%PDF-1.4"
                | b"%PDF-1.5"
                | b"%PDF-1.6"
                | b"%PDF-1.7"
                | b"%PDF-2.0"
        )
    ) && matches!(bytes.get(8), Some(b'\r' | b'\n'))
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
    fn pdf_header_controls_blob_mime_and_preserves_all_bytes() {
        for header in [b"%PDF-1.7\r\n".as_slice(), b"%PDF-2.0\n"] {
            let mut bytes = header.to_vec();
            bytes.extend(b"%\xff\0\x80binary\n%%EOF\n");
            let pdf = Attachment::from_bytes("wrong.txt".into(), &bytes).unwrap();
            assert_eq!(pdf.mime(), "application/pdf");
            assert_eq!(pdf.bytes, bytes.len());
            assert!(pdf.require_advertisement(false, false, true).is_ok());
            assert!(pdf.require_advertisement(true, true, false).is_err());
            assert!(!pdf.is_image());
            let v1::ContentBlock::Resource(resource) = pdf.content() else {
                panic!("resource")
            };
            let v1::EmbeddedResourceResource::BlobResourceContents(blob) = resource.resource else {
                panic!("blob")
            };
            assert_eq!(STANDARD.decode(blob.blob).unwrap(), bytes);
            assert_eq!(blob.mime_type.as_deref(), Some("application/pdf"));
            assert!(blob.uri.starts_with("attachment:///pdf/"));
        }
        for bytes in [
            b"%PDF-".as_slice(),
            b"%PDF-1.7",
            b"%PDF-9.9\n",
            b"plain text",
            b"%PDF-1.7oops",
        ] {
            assert!(Attachment::from_bytes("bad.pdf".into(), bytes).is_err());
        }
        let mut oversized = b"%PDF-1.7\n".to_vec();
        oversized.resize(MAX_ATTACHMENT_BYTES + 1, 0);
        assert!(Attachment::from_bytes("large.pdf".into(), &oversized).is_err());
    }
    #[tokio::test]
    async fn native_text_file_metadata_keeps_escaped_absolute_uri() {
        let media = Attachment::from_file(dioxus::html::FileData::new(
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
        assert!(
            matches!(media.content(), v1::ContentBlock::Resource(resource) if matches!(&resource.resource, v1::EmbeddedResourceResource::TextResourceContents(text) if text.uri == "file:///tmp/a%20%23%3F.rs"))
        );
    }
    #[tokio::test]
    async fn native_pdf_uri_is_escaped_and_browser_pdf_uris_are_distinct() {
        let pdf = Attachment::from_file(dioxus::html::FileData::new(
            dioxus::html::SerializedFileData {
                path: "/tmp/a #?.pdf".into(),
                size: 9,
                last_modified: 0,
                content_type: None,
                contents: Some(b"%PDF-1.7\n".to_vec().into()),
            },
        ))
        .await
        .unwrap();
        assert!(matches!(pdf.preview(), AttachmentPreview::Pdf));
        assert!(
            matches!(pdf.content(),v1::ContentBlock::Resource(resource) if matches!(&resource.resource,v1::EmbeddedResourceResource::BlobResourceContents(blob) if blob.uri=="file:///tmp/a%20%23%3F.pdf"))
        );
        let a = Attachment::from_bytes("a #?.pdf".into(), b"%PDF-1.7\n").unwrap();
        let b = Attachment::from_bytes("a #?.pdf".into(), b"%PDF-1.7\n").unwrap();
        assert_ne!(a.content(), b.content());
        assert!(
            matches!(a.content(),v1::ContentBlock::Resource(resource) if matches!(&resource.resource,v1::EmbeddedResourceResource::BlobResourceContents(blob) if blob.uri.ends_with("a%20%23%3F.pdf")))
        );
    }
    #[test]
    fn text_is_literal_utf8_with_distinct_escaped_uris_and_a_bounded_preview() {
        let bytes = "\u{feff}<script>é</script>\r\n\t".as_bytes();
        let first = Attachment::from_bytes("a #?.rs".into(), bytes).unwrap();
        let second = Attachment::from_bytes("a #?.rs".into(), bytes).unwrap();
        assert_eq!(first.bytes, bytes.len());
        assert!(first.require_advertisement(false, false, true).is_ok());
        assert!(first.require_advertisement(true, true, false).is_err());
        assert_ne!(first.content(), second.content());
        let v1::ContentBlock::Resource(resource) = first.content() else {
            panic!("resource")
        };
        let v1::EmbeddedResourceResource::TextResourceContents(text) = resource.resource else {
            panic!("text")
        };
        assert_eq!(text.text.as_bytes(), bytes);
        assert_eq!(text.mime_type.as_deref(), Some("text/plain"));
        assert!(text.uri.ends_with("a%20%23%3F.rs"));
        assert!(
            matches!(Attachment::from_bytes("large.txt".into(), "é".repeat(3000).as_bytes()).unwrap().preview(), AttachmentPreview::Text { excerpt } if excerpt.chars().count() == 2000)
        );
        assert!(
            matches!(Attachment::from_bytes("empty.txt".into(), b"").unwrap().preview(), AttachmentPreview::Text { excerpt } if excerpt.is_empty())
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
            assert!(Attachment::from_bytes("x.txt".into(), bytes).is_err());
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
            let attachment = Attachment::from_bytes("wrong.png".into(), bytes).unwrap();
            assert_eq!(attachment.mime(), mime);
            assert!(attachment.require_advertisement(false, true, false).is_ok());
            assert!(attachment.require_advertisement(true, false, true).is_err());
            let v1::ContentBlock::Audio(audio) = attachment.content() else {
                panic!("audio block")
            };
            assert_eq!(STANDARD.decode(audio.data).unwrap(), bytes);
            assert_eq!(audio.mime_type, mime);
        }
        assert!(Attachment::from_bytes("wrong.mp3".into(), b"\xff\xff\xff\xff").is_err());
        assert!(Attachment::from_bytes("aac.mp3".into(), b"\xff\xf1\x50\x80").is_err());
        for bytes in [
            b"ID3\x04\0\0\0\0\0\0\xff\xf1\x50\x80".as_slice(),
            b"ID3\x04\0\0\0\0\0\0",
            b"ID3\x04\0\0\x7f\x7f\x7f\x7f\xff\xfb\x90\x64",
        ] {
            assert!(Attachment::from_bytes("tagged.mp3".into(), bytes).is_err());
        }
    }
    #[test]
    fn signature_controls_mime_and_encoded_content_preserves_every_byte() {
        let bytes = b"\x89PNG\r\n\x1a\nunchanged";
        let image = Attachment::from_bytes("misnamed.jpg".into(), bytes).unwrap();
        assert_eq!(image.mime(), "image/png");
        assert!(image.require_advertisement(true, false, false).is_ok());
        assert!(image.require_advertisement(false, true, true).is_err());
        let v1::ContentBlock::Image(content) = image.content() else {
            panic!("image")
        };
        assert_eq!(STANDARD.decode(content.data).unwrap(), bytes);
        assert!(
            matches!(image.preview(), AttachmentPreview::Image { src } if src.starts_with("data:image/png;base64,"))
        );
    }
    #[test]
    fn unsupported_or_oversized_selections_fail_without_an_attachment() {
        assert!(Attachment::from_bytes("x.png".into(), b"not an image").is_err());
        let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
        bytes.resize(MAX_ATTACHMENT_BYTES + 1, 0);
        assert!(Attachment::from_bytes("x".into(), &bytes).is_err());
    }
}
