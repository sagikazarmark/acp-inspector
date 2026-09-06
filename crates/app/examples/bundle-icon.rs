//! The bundle's icon, one size at a time.
//!
//!     cargo run -p acp-inspector --example bundle-icon -- 1024 icon.png
//!
//! Renders the mark the window draws (`src/mark.rs`) on macOS's icon grid, at
//! the size asked for, as a PNG. `just icon` runs this at the ten sizes an
//! `.icns` holds and has `iconutil` fold them into `crates/app/bundle/icon.icns`
//! ([ADR 0010](../../../docs/adr/0010-the-bundle-is-the-one-thing-dx-makes.md)).
//! Nothing the program is built from touches this file: it is packaging.
//!
//! An example rather than a second binary, so that `cargo run -p acp-inspector`
//! and `dx bundle` still see one program; and the drawing is included by path
//! because that program has no library for an example to import it from.

use std::error::Error;
use std::fs::File;
use std::io::BufWriter;
use std::process::ExitCode;

#[path = "../src/mark.rs"]
mod mark;

/// macOS's icon grid: on a 1024px canvas the rounded square is 824px, with
/// 100px clear on every side, and every smaller rendition keeps the proportion.
/// An icon that fills its canvas sits visibly oversized in the Dock.
const MACOS_GRID_MARGIN: f32 = 100.0 / 1024.0;

fn main() -> ExitCode {
    let mut arguments = std::env::args().skip(1);
    let (Some(side), Some(out), None) = (arguments.next(), arguments.next(), arguments.next())
    else {
        eprintln!("usage: bundle-icon SIDE OUT.png");
        return ExitCode::from(2);
    };
    match render(&side, &out) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("bundle-icon: {error}");
            ExitCode::FAILURE
        }
    }
}

fn render(side: &str, out: &str) -> Result<(), Box<dyn Error>> {
    let side: u32 = side.parse()?;
    let rgba = mark::rgba(side, MACOS_GRID_MARGIN);

    let mut encoder = png::Encoder::new(BufWriter::new(File::create(out)?), side, side);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&rgba)?;
    writer.finish()?;
    Ok(())
}
