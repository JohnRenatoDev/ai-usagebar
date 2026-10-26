//! The `copy-image` IPC's validation. The popover's page serializes the selected provider's
//! card to a PNG and posts its base64 (no `data:` prefix); the host puts those bytes on the
//! system clipboard. A pure function over the received JSON, compiled on every OS like
//! `blur` so its tests run on every CI job; only the tray hosts call it.

use std::io::Cursor;

use base64::Engine as _;
use serde_json::Value;

/// The PNG file signature: `\x89PNG\r\n\x1a\n`.
const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];

/// The ceiling for the base64 `png` field, in bytes. Base64 inflates by ~4/3, so past this
/// the decoded PNG is already over 12 MiB — far beyond anything the card renders.
const MAX_ENCODED_BYTES: usize = 16 * 1024 * 1024;

/// The ceiling for the PNG's width and height, in pixels (from the IHDR header).
const MAX_DIMENSION_PX: u32 = 8192;

/// Validates a `copy-image` command and returns the PNG's bytes.
///
/// The `png` field must be a string within [`MAX_ENCODED_BYTES`], decode under the standard
/// base64 alphabet, and carry the PNG signature; its width and height are read from the IHDR
/// header (via the `png` crate) and must fit within [`MAX_DIMENSION_PX`]. Each refusal is a
/// short, stable English message: the host turns any `Err` into `false` for the page.
pub(crate) fn png_from_ipc(value: &Value) -> Result<Vec<u8>, &'static str> {
    let Some(encoded) = value.get("png").and_then(Value::as_str) else {
        return Err("the `png` field is missing or not a string");
    };
    if encoded.len() > MAX_ENCODED_BYTES {
        return Err("the base64 `png` field exceeds 16 MiB");
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| "the `png` field is not valid base64")?;
    if !bytes.starts_with(&PNG_SIGNATURE) {
        return Err("the bytes are not a PNG (bad signature)");
    }
    let mut decoder = png::Decoder::new(Cursor::new(bytes.as_slice()));
    let header = decoder
        .read_header_info()
        .map_err(|_| "the PNG header (IHDR) could not be read")?;
    if header.width > MAX_DIMENSION_PX || header.height > MAX_DIMENSION_PX {
        return Err("the PNG is wider or taller than 8192 px");
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD;

    /// A real `width`×`height` RGBA PNG, encoded with the same crate the validator reads.
    fn valid_png(width: u32, height: u32) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, width, height);
            encoder.set_color(png::ColorType::Rgba);
            let mut writer = encoder.write_header().expect("header encodes");
            // The pixel contents are irrelevant to the validator; all zero is a valid RGBA row.
            let pixels = vec![0u8; (width * height * 4) as usize];
            writer.write_image_data(&pixels).expect("image encodes");
        }
        out
    }

    fn command(png: &str) -> Value {
        serde_json::json!({ "cmd": "copy-image", "png": png })
    }

    #[test]
    fn a_minimal_valid_png_passes() {
        let bytes = valid_png(1, 1);
        let encoded = STANDARD.encode(&bytes);
        let got = png_from_ipc(&command(&encoded)).expect("the generated PNG is accepted");
        assert_eq!(got, bytes);
    }

    #[test]
    fn the_dimension_ceiling_is_inclusive() {
        // 8192 still passes on both axes; the over-limit cases are the next test.
        for (w, h) in [(8192, 1), (1, 8192)] {
            let encoded = STANDARD.encode(valid_png(w, h));
            png_from_ipc(&command(&encoded)).expect("8192 px is within the limit");
        }
    }

    #[test]
    fn a_missing_or_non_string_field_is_rejected() {
        assert!(png_from_ipc(&serde_json::json!({ "cmd": "copy-image" })).is_err());
        assert!(png_from_ipc(&command("")).is_err());
        assert!(png_from_ipc(&serde_json::json!({ "cmd": "copy-image", "png": null })).is_err());
        assert!(png_from_ipc(&serde_json::json!({ "cmd": "copy-image", "png": 42 })).is_err());
    }

    #[test]
    fn invalid_base64_is_rejected() {
        assert!(png_from_ipc(&command("not-base64!!")).is_err());
    }

    #[test]
    fn an_overlong_base64_field_is_rejected() {
        let long = "A".repeat(MAX_ENCODED_BYTES + 1);
        assert!(png_from_ipc(&command(&long)).is_err());
    }

    #[test]
    fn bytes_without_the_png_signature_are_rejected() {
        let plain = STANDARD.encode(b"hello world, not a png at all");
        assert!(png_from_ipc(&command(&plain)).is_err());
    }

    #[test]
    fn a_signature_with_a_truncated_header_is_rejected() {
        let mut truncated = PNG_SIGNATURE.to_vec();
        truncated.push(0);
        assert!(png_from_ipc(&command(&STANDARD.encode(&truncated))).is_err());
    }

    #[test]
    fn a_too_wide_or_too_tall_png_is_rejected() {
        let too_wide = STANDARD.encode(valid_png(8193, 1));
        assert!(png_from_ipc(&command(&too_wide)).is_err());
        let too_tall = STANDARD.encode(valid_png(1, 8193));
        assert!(png_from_ipc(&command(&too_tall)).is_err());
    }
}
