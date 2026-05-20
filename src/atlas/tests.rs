#[cfg(any(feature = "builder", feature = "png_source"))]
use super::*;
#[cfg(any(feature = "builder", feature = "png_source"))]
use png::Encoder;
#[cfg(any(feature = "builder", feature = "png_source"))]
use std::fmt::Write;

#[cfg(any(feature = "builder", feature = "png_source"))]
fn encode_png(color_type: ColorType, data: &[u8], width: u32, height: u32, palette: Option<&[u8]>) -> Vec<u8> {
    let mut buffer = Vec::new();
    {
        let mut encoder = Encoder::new(&mut buffer, width, height);
        encoder.set_color(color_type);
        encoder.set_depth(BitDepth::Eight);
        if let Some(palette) = palette {
            encoder.set_palette(palette);
        }
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(data).unwrap();
    }
    buffer
}

#[cfg(any(feature = "builder", feature = "png_source"))]
#[test]
fn png_decode_error_returns_err() {
    let res = load_image_bytes(ImageSource::Png { bytes: &[] });
    assert!(res.is_err());
}

#[cfg(any(feature = "builder", feature = "png_source"))]
#[test]
fn png_decode_rgb_expands_alpha() {
    let bytes = encode_png(ColorType::Rgb, &[10, 20, 30], 1, 1, None);
    let (width, height, pixels) = load_image_bytes(ImageSource::Png { bytes: &bytes }).unwrap();

    assert_eq!(width, 1);
    assert_eq!(height, 1);
    assert_eq!(pixels.len(), 1);
    let pixel = pixels[0];
    assert_eq!((pixel.x, pixel.y, pixel.z, pixel.w), (10, 20, 30, 0xFF));
}

#[cfg(any(feature = "builder", feature = "png_source"))]
#[test]
fn png_decode_indexed_uses_palette() {
    let palette = [0x01, 0x02, 0x03];
    let bytes = encode_png(ColorType::Indexed, &[0], 1, 1, Some(&palette));
    let (width, height, pixels) = load_image_bytes(ImageSource::Png { bytes: &bytes }).unwrap();

    assert_eq!(width, 1);
    assert_eq!(height, 1);
    assert_eq!(pixels.len(), 1);

    let pixel = pixels[0];
    let mut message = String::new();
    let _ = write!(&mut message, "{},{},{},{}", pixel.x, pixel.y, pixel.z, pixel.w);
    assert_eq!(message, "1,2,3,255");
}
