//! Writes the exe icon, `crates/app/assets/shelve.ico`, from the drawing in `icon.rs`.
//! Run after changing the drawing: cargo run -p shelve-platform --example make_ico

use std::path::Path;

use shelve_platform::{icon_rgba, IconStyle};

/// Sizes Explorer asks for, small to large. 256 is stored as PNG, the rest as BMP.
const SIZES: [u32; 5] = [16, 24, 32, 48, 256];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let images: Vec<(u32, Vec<u8>)> = SIZES
        .into_iter()
        .map(|size| {
            let rgba = icon_rgba(IconStyle::Tile, size);
            let data = if size == 256 {
                png(&rgba, size)
            } else {
                Ok(bmp(&rgba, size))
            };
            data.map(|d| (size, d))
        })
        .collect::<Result<_, _>>()?;

    // ICONDIR, then one 16-byte ICONDIRENTRY per image, then the images.
    let mut ico = Vec::new();
    ico.extend(
        [0u16, 1, images.len() as u16]
            .iter()
            .flat_map(|v| v.to_le_bytes()),
    );
    let mut offset = 6 + 16 * images.len() as u32;
    for (size, data) in &images {
        let side = if *size == 256 { 0 } else { *size as u8 };
        ico.extend([side, side, 0, 0]);
        ico.extend([1u16, 32].iter().flat_map(|v| v.to_le_bytes())); // planes, bits per pixel
        ico.extend((data.len() as u32).to_le_bytes());
        ico.extend(offset.to_le_bytes());
        offset += data.len() as u32;
    }
    for (_, data) in &images {
        ico.extend(data);
    }

    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../app/assets/shelve.ico");
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, &ico)?;
    println!("{} bytes -> {}", ico.len(), path.display());
    Ok(())
}

/// Icon flavour of BMP: no file header, height doubled, rows bottom-up as BGRA,
/// followed by a 1-bit mask (all zero: transparency comes from the alpha channel).
fn bmp(rgba: &[u8], size: u32) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend(40u32.to_le_bytes());
    out.extend((size as i32).to_le_bytes());
    out.extend((size as i32 * 2).to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(32u16.to_le_bytes());
    out.extend([0u8; 24]); // no compression, sizes and palette left to the reader
    for row in rgba.chunks((size * 4) as usize).rev() {
        for p in row.chunks(4) {
            out.extend([p[2], p[1], p[0], p[3]]);
        }
    }
    let mask_row = size.div_ceil(32) * 4;
    out.resize(out.len() + (mask_row * size) as usize, 0);
    out
}

fn png(rgba: &[u8], size: u32) -> Result<Vec<u8>, png::EncodingError> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, size, size);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()?.write_image_data(rgba)?;
    Ok(out)
}
