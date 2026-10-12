//! The bundles' icons, one file at a time.
//!
//!     cargo run -p acp-inspector --example bundle-icon -- 1024 icon.png
//!     cargo run -p acp-inspector --example bundle-icon -- --edge 256 256x256.png
//!     cargo run -p acp-inspector --example bundle-icon -- --ico icon.ico
//!
//! Renders the mark the window draws (`src/mark.rs`). The plain form puts it on
//! macOS's icon grid, at the size asked for, as a PNG: `just icon` runs it at
//! the ten sizes an `.icns` holds and has `iconutil` fold them into
//! `crates/app/bundle/icon.icns`. `--edge` fills the canvas, as the window's own
//! icon does, for the Linux package's icon theme; `--ico` writes the same at the
//! sizes Windows asks an `.ico` for, for the executable and its installer
//! (ADR 0010, `docs/adr/0010-the-bundle-is-the-one-thing-dx-makes.md`).
//! Nothing the program is built from touches this file: it is packaging.
//!
//! An example rather than a second binary, so that `cargo run -p acp-inspector`
//! and `dx bundle` still see one program; and the drawing is included by path
//! because that program has no library for an example to import it from.

use std::error::Error;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::process::ExitCode;

#[path = "../src/mark.rs"]
mod mark;

/// macOS's icon grid: on a 1024px canvas the rounded square is 824px, with
/// 100px clear on every side, and every smaller rendition keeps the proportion.
/// An icon that fills its canvas sits visibly oversized in the Dock.
const MACOS_GRID_MARGIN: f32 = 100.0 / 1024.0;

/// The sizes an `.ico` carries: the shell picks the nearest for each view,
/// from the 16px title bar to the 256px tile.
const ICO_SIDES: [u32; 6] = [16, 24, 32, 48, 64, 256];

const USAGE: &str = "usage: bundle-icon [--edge] SIDE OUT.png | bundle-icon --ico OUT.ico";

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let arguments: Vec<&str> = arguments.iter().map(String::as_str).collect();
    let result = match arguments.as_slice() {
        ["--ico", out] => ico(out),
        ["--edge", side, out] => png(side, 0.0, out),
        [side, out] => png(side, MACOS_GRID_MARGIN, out),
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("bundle-icon: {error}");
            ExitCode::FAILURE
        }
    }
}

fn png(side: &str, margin: f32, out: &str) -> Result<(), Box<dyn Error>> {
    let side: u32 = side.parse()?;
    File::create(out)?.write_all(&encode(side, margin)?)?;
    Ok(())
}

/// An `.ico` whose images are PNGs, which every Windows since Vista reads:
/// a six-byte header, a sixteen-byte entry per image, then the images.
fn ico(out: &str) -> Result<(), Box<dyn Error>> {
    let images = ICO_SIDES
        .iter()
        .map(|&side| encode(side, 0.0))
        .collect::<Result<Vec<_>, _>>()?;

    let mut file = BufWriter::new(File::create(out)?);
    let count = images.len() as u16;
    file.write_all(&0u16.to_le_bytes())?; // reserved
    file.write_all(&1u16.to_le_bytes())?; // 1: an icon, not a cursor
    file.write_all(&count.to_le_bytes())?;

    let mut offset = 6 + 16 * u32::from(count);
    for (&side, image) in ICO_SIDES.iter().zip(&images) {
        // A side of 256 does not fit the byte, and is written as 0.
        let byte = if side >= 256 { 0 } else { side as u8 };
        file.write_all(&[byte, byte, 0, 0])?; // width, height, no palette, reserved
        file.write_all(&1u16.to_le_bytes())?; // colour planes
        file.write_all(&32u16.to_le_bytes())?; // bits per pixel
        file.write_all(&(image.len() as u32).to_le_bytes())?;
        file.write_all(&offset.to_le_bytes())?;
        offset += image.len() as u32;
    }
    for image in &images {
        file.write_all(image)?;
    }
    file.flush()?;
    Ok(())
}

fn encode(side: u32, margin: f32) -> Result<Vec<u8>, Box<dyn Error>> {
    let rgba = mark::rgba(side, margin);
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, side, side);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&rgba)?;
    writer.finish()?;
    Ok(bytes)
}
