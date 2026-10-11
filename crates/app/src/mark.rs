//! The tool's mark, drawn procedurally so the build needs no binary asset
//! (the same argument as `docs/adr/0003-the-stylesheet-is-generated-and-committed.md`).
//!
//! Two bars crossing a rounded square in opposite directions: a frame going out
//! and one coming back, in the Trace's direction colours on the accent.
//! Proportions are in sixty-fourths and edges are drawn by coverage, so one
//! function serves the 64px window icon (`shell::icon`) and the bundle icon's
//! sizes (`examples/bundle-icon.rs`). `std` only, so the example can include it
//! by path.

/// The grid the mark is drawn on: every proportion below is in these units.
const GRID: f32 = 64.0;

/// The ground: the accent, rounded at the window's control-box radius.
const GROUND: [u8; 3] = [37, 78, 210];
const RADIUS: f32 = 14.0;

/// The two directions, in the Trace's colours.
const OUT: [u8; 3] = [244, 186, 96];
const BACK: [u8; 3] = [126, 200, 255];

/// Both bars span the same two columns; the head's tip is at the end the bar
/// points at.
const BAR_FROM: f32 = 14.0;
const BAR_TO: f32 = 50.0;
const BAR_THICKNESS: f32 = 6.0;
/// The out bar sits as far above the middle as the back bar sits below it.
const OUT_CENTRE: f32 = 25.0;
const BACK_CENTRE: f32 = 39.0;
/// The head is a right angle at the tip.
const HEAD: f32 = 8.0;

/// Coverage samples per pixel, per axis. An edge's alpha is the share of these
/// that fell on the shape, so this is how many steps an edge has.
const SAMPLES: u32 = 8;

/// The mark as straight (non-premultiplied) RGBA, `side` pixels a side.
///
/// `margin` is the fraction of the side left clear on every edge: `0.0` fills
/// the canvas, as a window icon does; macOS's icon grid leaves 100/1024.
pub fn rgba(side: u32, margin: f32) -> Vec<u8> {
    let mut rgba = vec![0u8; (side * side * 4) as usize];
    let inset = side as f32 * margin;
    // The mark's side in pixels, and so how many pixels one grid unit is.
    let unit = (side as f32 - 2.0 * inset) / GRID;
    let samples = (SAMPLES * SAMPLES) as f32;

    for y in 0..side {
        for x in 0..side {
            // Nested counts: the bars lie wholly on the ground, and a sample
            // is on at most one bar.
            let (mut ground, mut out, mut back) = (0u32, 0u32, 0u32);
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let px = x as f32 + (sx as f32 + 0.5) / SAMPLES as f32;
                    let py = y as f32 + (sy as f32 + 0.5) / SAMPLES as f32;
                    let (u, v) = ((px - inset) / unit, (py - inset) / unit);
                    if !on_ground(u, v) {
                        continue;
                    }
                    ground += 1;
                    if on_bar(u, v, OUT_CENTRE, true) {
                        out += 1;
                    } else if on_bar(u, v, BACK_CENTRE, false) {
                        back += 1;
                    }
                }
            }
            if ground == 0 {
                continue;
            }

            let (weight_out, weight_back) =
                (out as f32 / ground as f32, back as f32 / ground as f32);
            let weight_ground = 1.0 - weight_out - weight_back;
            let at = ((y * side + x) * 4) as usize;
            for (channel, pixel) in rgba[at..at + 3].iter_mut().enumerate() {
                let mixed = GROUND[channel] as f32 * weight_ground
                    + OUT[channel] as f32 * weight_out
                    + BACK[channel] as f32 * weight_back;
                *pixel = mixed.round() as u8;
            }
            rgba[at + 3] = (255.0 * ground as f32 / samples).round() as u8;
        }
    }

    rgba
}

/// Whether a grid point is on the rounded square.
fn on_ground(u: f32, v: f32) -> bool {
    let dx = (RADIUS - u).max(u - (GRID - RADIUS)).max(0.0);
    let dy = (RADIUS - v).max(v - (GRID - RADIUS)).max(0.0);
    dx * dx + dy * dy <= RADIUS * RADIUS
}

/// Whether a point of the grid is on a bar — its shaft or its head — centred
/// on the row `centre`, pointing right if `rightwards` and otherwise left.
fn on_bar(u: f32, v: f32, centre: f32, rightwards: bool) -> bool {
    // The back bar is the out bar mirrored between the shared columns.
    let u = if rightwards { u } else { BAR_FROM + BAR_TO - u };
    let from_centre = (v - centre).abs();
    let shaft = (BAR_FROM..=BAR_TO).contains(&u) && from_centre <= BAR_THICKNESS / 2.0;
    let head = (BAR_TO - HEAD..=BAR_TO).contains(&u) && from_centre <= BAR_TO - u;
    shaft || head
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(rgba: &[u8], side: u32, x: u32, y: u32) -> [u8; 4] {
        let at = ((y * side + x) * 4) as usize;
        rgba[at..at + 4].try_into().unwrap()
    }

    fn opaque(colour: [u8; 3]) -> [u8; 4] {
        [colour[0], colour[1], colour[2], 255]
    }

    #[test]
    fn the_ground_fills_the_canvas_with_the_corners_off() {
        let side = 64;
        let mark = rgba(side, 0.0);
        assert_eq!(mark.len(), (side * side * 4) as usize);
        assert_eq!(pixel(&mark, side, 32, 32), opaque(GROUND), "the middle");
        assert_eq!(pixel(&mark, side, 32, 0), opaque(GROUND), "the top edge");
        assert_eq!(pixel(&mark, side, 0, 32), opaque(GROUND), "the left edge");
        assert_eq!(pixel(&mark, side, 0, 0)[3], 0, "a corner is clear");
        assert_eq!(pixel(&mark, side, 63, 63)[3], 0, "every corner is clear");
        // A pixel on the curve is partly covered and still the ground's colour.
        let on_the_curve = pixel(&mark, side, 2, 5);
        assert!(
            0 < on_the_curve[3] && on_the_curve[3] < 255,
            "the corner is a curve, not a step: {on_the_curve:?}"
        );
        assert_eq!(on_the_curve[..3], GROUND);
    }

    #[test]
    fn a_margin_leaves_the_edge_clear_and_the_mark_in_the_middle() {
        let side = 1024;
        let mark = rgba(side, 100.0 / 1024.0);
        assert_eq!(pixel(&mark, side, 50, 512)[3], 0, "the margin is clear");
        assert_eq!(pixel(&mark, side, 512, 50)[3], 0, "on every side");
        assert_eq!(
            pixel(&mark, side, 101, 512),
            opaque(GROUND),
            "the ground starts past it"
        );
        assert_eq!(
            pixel(&mark, side, 922, 512),
            opaque(GROUND),
            "and ends before the far one"
        );
        assert_eq!(pixel(&mark, side, 924, 512)[3], 0);
    }

    #[test]
    fn one_bar_goes_out_to_the_right_and_the_other_comes_back_to_the_left() {
        let side = 64;
        let mark = rgba(side, 0.0);
        // The shafts, at both ends and in the middle.
        for x in [15, 32, 48] {
            assert_eq!(pixel(&mark, side, x, 25), opaque(OUT), "the out bar at {x}");
            assert_eq!(
                pixel(&mark, side, x, 39),
                opaque(BACK),
                "the back bar at {x}"
            );
        }
        // The head is at the end the bar points to.
        assert_eq!(
            pixel(&mark, side, 42, 20),
            opaque(OUT),
            "the out head's base"
        );
        assert_eq!(
            pixel(&mark, side, 15, 20),
            opaque(GROUND),
            "no head where it came from"
        );
        assert_eq!(
            pixel(&mark, side, 21, 34),
            opaque(BACK),
            "the back head's base"
        );
        assert_eq!(
            pixel(&mark, side, 48, 34),
            opaque(GROUND),
            "no head where it came from"
        );
        // And a head tapers to its tip at the bar's end.
        assert_eq!(pixel(&mark, side, 48, 20), opaque(GROUND), "past the taper");
        assert_eq!(pixel(&mark, side, 15, 34), opaque(GROUND), "past the taper");
    }

    #[test]
    fn it_is_the_same_drawing_at_every_size() {
        // Twice the size averaged down matches the drawing at the size.
        // Premultiplied, so a clear pixel's colour counts for nothing.
        fn premultiplied(pixel: [u8; 4]) -> [f32; 4] {
            let alpha = pixel[3] as f32 / 255.0;
            [
                pixel[0] as f32 * alpha,
                pixel[1] as f32 * alpha,
                pixel[2] as f32 * alpha,
                pixel[3] as f32,
            ]
        }
        let (small, large) = (64u32, 128u32);
        let at_small = rgba(small, 0.0);
        let at_large = rgba(large, 0.0);
        let mut worst = 0f32;
        for y in 0..small {
            for x in 0..small {
                let expected = premultiplied(pixel(&at_small, small, x, y));
                let quarters = [(0, 0), (1, 0), (0, 1), (1, 1)]
                    .map(|(dx, dy)| premultiplied(pixel(&at_large, large, 2 * x + dx, 2 * y + dy)));
                for channel in 0..4 {
                    let averaged =
                        quarters.iter().map(|quarter| quarter[channel]).sum::<f32>() / 4.0;
                    worst = worst.max((expected[channel] - averaged).abs());
                }
            }
        }
        // Sample positions differ between sizes, so allow a sample or two.
        let one_sample = 255.0 / (SAMPLES * SAMPLES) as f32;
        assert!(
            worst <= 2.0 * one_sample,
            "the two sizes disagree by {worst}/255 somewhere"
        );
    }
}
