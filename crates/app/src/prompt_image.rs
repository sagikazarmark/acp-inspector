//! One selected image, encoded once without changing the original bytes.
use acp_inspector_core::v1;
use base64::{Engine, engine::general_purpose::STANDARD};

pub const MAX_IMAGE_BYTES: usize = 5 * 1024 * 1024;

#[derive(Clone, PartialEq)]
pub struct PromptImage {
    pub name: String,
    pub bytes: usize,
    pub mime: &'static str,
    data: String,
}

impl PromptImage {
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
                    return Err("Choose a regular image file.".into());
                }
                let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
                if !file.metadata().map_err(|e| e.to_string())?.is_file() {
                    return Err("Choose a regular image file.".into());
                }
                let mut bytes = Vec::new();
                file.take((MAX_IMAGE_BYTES + 1) as u64)
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
            return Err("Choose a readable regular image file.".into());
        };
        #[cfg(target_arch = "wasm32")]
        let bytes = {
            if file.size() > MAX_IMAGE_BYTES as u64 {
                return Err("An image may be at most 5 MiB.".into());
            }
            file.read_bytes().await.map_err(|e| e.to_string())?.to_vec()
        };
        Self::from_bytes(name, &bytes)
    }
    pub fn from_bytes(name: String, bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_IMAGE_BYTES {
            return Err("An image may be at most 5 MiB.".into());
        }
        let mime = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            "image/png"
        } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
            "image/jpeg"
        } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
            "image/gif"
        } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
            "image/webp"
        } else {
            return Err(
                "Choose a PNG, JPEG, GIF or WebP image (identified by its file signature).".into(),
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
        v1::ContentBlock::Image(v1::ImageContent::new(self.data.clone(), self.mime))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signature_controls_mime_and_encoded_content_preserves_every_byte() {
        let bytes = b"\x89PNG\r\n\x1a\nunchanged";
        let image = PromptImage::from_bytes("misnamed.jpg".into(), bytes).unwrap();
        assert_eq!(image.mime, "image/png");
        let v1::ContentBlock::Image(content) = image.content() else {
            panic!("image")
        };
        assert_eq!(STANDARD.decode(content.data).unwrap(), bytes);
        assert!(image.preview().starts_with("data:image/png;base64,"));
    }
    #[test]
    fn unsupported_or_oversized_selections_fail_without_an_attachment() {
        assert!(PromptImage::from_bytes("x.png".into(), b"not an image").is_err());
        let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
        bytes.resize(MAX_IMAGE_BYTES + 1, 0);
        assert!(PromptImage::from_bytes("x".into(), &bytes).is_err());
    }
}
