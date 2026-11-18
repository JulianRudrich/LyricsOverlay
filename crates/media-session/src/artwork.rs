//! Cover art that a player hands over as bytes, turned into a `data:` URL the
//! overlay can show without another request.

/// Larger pictures are left out, so the URL stays small enough to send with
/// every reading.
pub(crate) const MAX_BYTES: usize = 512 * 1024;

/// A `data:` URL for a JPEG, PNG, GIF, WebP or BMP picture, or `None` for
/// anything else, nothing at all and pictures over [`MAX_BYTES`]. The type
/// comes from the bytes themselves, because players do not always name it.
pub(crate) fn image_data_url(bytes: &[u8]) -> Option<String> {
    if bytes.len() > MAX_BYTES {
        return None;
    }
    let mime = if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "image/jpeg"
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        "image/gif"
    } else if bytes.len() > 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else if bytes.starts_with(b"BM") && bytes.len() > 14 {
        "image/bmp"
    } else {
        return None;
    };
    Some(format!("data:{mime};base64,{}", base64(bytes)))
}

/// Standard base64 with padding.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut text = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let group = chunk
            .iter()
            .enumerate()
            .fold(0u32, |group, (index, &byte)| {
                group | (u32::from(byte) << (16 - 8 * index))
            });
        for index in 0..4 {
            if index <= chunk.len() {
                let digit = (group >> (18 - 6 * index)) & 0x3F;
                text.push(char::from(ALPHABET[digit as usize]));
            } else {
                text.push('=');
            }
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_standard_vectors() {
        let vectors = [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ];
        for (plain, encoded) in vectors {
            assert_eq!(base64(plain.as_bytes()), encoded, "{plain:?}");
        }
        assert_eq!(base64(&[0xFB, 0xFF, 0xBF]), "+/+/");
    }

    #[test]
    fn pictures_get_their_type_from_their_bytes() {
        let png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";
        assert_eq!(
            image_data_url(png).as_deref(),
            Some("data:image/png;base64,iVBORw0KGgoAAAANSUhEUg==")
        );
        let jpeg = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        assert!(
            image_data_url(&jpeg).is_some_and(|url| url.starts_with("data:image/jpeg;base64,/9j/"))
        );
        let webp = b"RIFF\x24\0\0\0WEBPVP8 ";
        assert!(image_data_url(webp).is_some_and(|url| url.starts_with("data:image/webp;")));
    }

    #[test]
    fn anything_else_is_left_out() {
        assert_eq!(image_data_url(b""), None);
        assert_eq!(
            image_data_url(b"<svg xmlns='http://www.w3.org/2000/svg'/>"),
            None
        );
        let mut huge = vec![0xFF, 0xD8, 0xFF];
        huge.resize(MAX_BYTES + 1, 0);
        assert_eq!(image_data_url(&huge), None);
    }
}
