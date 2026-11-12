//! The Shelve icon as pixels: a stack of three cards, each peeking out above
//! and to the right of the one in front. Pure drawing, no Windows code.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconStyle {
    /// White drawing on a rounded blue tile: Explorer, title bars.
    Tile,
    /// Drawing only, white: for a dark taskbar.
    MonoWhite,
    /// Drawing only, near black: for a light taskbar.
    MonoDark,
}

/// x, y, w, h, corner radius.
type RoundRect = (f32, f32, f32, f32, f32);

/// Painted in order: a card adds ink, the gap around the next card clears it again.
struct Glyph {
    grid: f32,
    /// Back to front: (card, the gap ring drawn just before it, if any).
    cards: [(RoundRect, Option<RoundRect>); 3],
}

/// Drawn for 32 px and up, where there is room for proportions.
const LARGE: Glyph = Glyph {
    grid: 32.0,
    cards: [
        ((12.0, 5.0, 16.0, 12.0, 2.5), None),
        ((8.0, 10.0, 16.0, 12.0, 2.5), Some((6.5, 8.5, 19.0, 15.0, 4.0))),
        ((4.0, 15.0, 16.0, 12.0, 2.5), Some((2.5, 13.5, 19.0, 15.0, 4.0))),
    ],
};

/// The tile at 16 px, pixel by pixel: the large drawing shrunk this far blurs the gaps.
/// Its grid is the whole tile, so the margin is part of the coordinates.
const TILE_16: Glyph = Glyph {
    grid: 16.0,
    cards: [
        ((7.0, 4.0, 7.0, 5.0, 1.0), None),
        ((5.0, 6.0, 7.0, 5.0, 1.0), Some((4.0, 5.0, 9.0, 7.0, 1.5))),
        ((3.0, 8.0, 7.0, 5.0, 1.0), Some((2.0, 7.0, 9.0, 7.0, 1.5))),
    ],
};

/// The tile at 24 px, pixel by pixel, for the same reason.
const TILE_24: Glyph = Glyph {
    grid: 24.0,
    cards: [
        ((10.0, 5.0, 10.0, 7.0, 1.5), None),
        ((7.0, 8.0, 10.0, 7.0, 1.5), Some((6.0, 7.0, 12.0, 9.0, 2.0))),
        ((4.0, 11.0, 10.0, 7.0, 1.5), Some((3.0, 10.0, 12.0, 9.0, 2.0))),
    ],
};

/// Drawn pixel by pixel for the 16 px tray slot: whole-pixel edges, one-pixel gaps.
const SMALL: Glyph = Glyph {
    grid: 16.0,
    cards: [
        ((7.0, 2.0, 8.0, 6.0, 1.0), None),
        ((4.0, 5.0, 8.0, 6.0, 1.0), Some((3.0, 4.0, 10.0, 8.0, 1.5))),
        ((1.0, 8.0, 8.0, 6.0, 1.0), Some((0.0, 7.0, 10.0, 8.0, 1.5))),
    ],
};

const TILE: [u8; 3] = [0x1F, 0x4E, 0x79];
const TILE_RADIUS: f32 = 7.0 / 32.0;
/// Share of the tile side the drawing takes.
const TILE_FILL: f32 = 0.68;
const WHITE: [u8; 3] = [0xFF; 3];
const DARK: [u8; 3] = [0x1B; 3];
/// Samples per pixel side for anti-aliasing.
const SS: u32 = 4;

/// RGBA (straight alpha), `size` × `size`.
pub fn icon_rgba(style: IconStyle, size: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            out.extend(pixel(style, size, x, y));
        }
    }
    out
}

fn pixel(style: IconStyle, size: u32, x: u32, y: u32) -> [u8; 4] {
    // The pixel-tuned tiles already hold their margin; the large drawing gets shrunk.
    let (tile_glyph, fill) = match size {
        16 => (&TILE_16, 1.0),
        24 => (&TILE_24, 1.0),
        _ => (&LARGE, TILE_FILL),
    };
    let (mut ink, mut tile) = (0u32, 0u32);
    for sy in 0..SS {
        for sx in 0..SS {
            // Sample position in 0..1 of the icon side.
            let u = (x as f32 + (sx as f32 + 0.5) / SS as f32) / size as f32;
            let v = (y as f32 + (sy as f32 + 0.5) / SS as f32) / size as f32;
            match style {
                IconStyle::Tile => {
                    if in_rounded_square(u, v) {
                        tile += 1;
                        let shrink = |t: f32| (t - 0.5) / fill + 0.5;
                        ink += tile_glyph.covers(shrink(u), shrink(v)) as u32;
                    }
                }
                IconStyle::MonoWhite | IconStyle::MonoDark => ink += SMALL.covers(u, v) as u32,
            }
        }
    }
    let total = SS * SS;
    let alpha = |n: u32| (n * 255 / total) as u8;
    match style {
        IconStyle::Tile if tile == 0 => [0; 4],
        IconStyle::Tile => {
            let mix = |i: usize| {
                let (a, b) = (TILE[i] as u32, WHITE[i] as u32);
                ((a * (tile - ink) + b * ink) / tile) as u8
            };
            [mix(0), mix(1), mix(2), alpha(tile)]
        }
        IconStyle::MonoWhite => [WHITE[0], WHITE[1], WHITE[2], alpha(ink)],
        IconStyle::MonoDark => [DARK[0], DARK[1], DARK[2], alpha(ink)],
    }
}

fn in_rounded_square(u: f32, v: f32) -> bool {
    let r = TILE_RADIUS;
    let (cx, cy) = (u.clamp(r, 1.0 - r), v.clamp(r, 1.0 - r));
    (u - cx).powi(2) + (v - cy).powi(2) <= r * r
}

impl Glyph {
    /// `u`, `v` in 0..1 of the drawing's side.
    fn covers(&self, u: f32, v: f32) -> bool {
        let (x, y) = (u * self.grid, v * self.grid);
        self.cards.iter().fold(false, |ink, &(card, gap)| {
            let ink = ink && !gap.is_some_and(|g| inside(g, x, y));
            ink || inside(card, x, y)
        })
    }
}

fn inside((rx, ry, w, h, r): RoundRect, x: f32, y: f32) -> bool {
    if !(rx..rx + w).contains(&x) || !(ry..ry + h).contains(&y) {
        return false;
    }
    // Nearest point of the rectangle shrunk by the radius: farther than `r` means a cut corner.
    let (cx, cy) = (x.clamp(rx + r, rx + w - r), y.clamp(ry + r, ry + h - r));
    (x - cx).powi(2) + (y - cy).powi(2) <= r * r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(rgba: &[u8], size: usize, x: usize, y: usize) -> [u8; 4] {
        let i = (y * size + x) * 4;
        [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
    }

    #[test]
    fn tile_is_a_rounded_blue_square_with_white_cards() {
        let t = icon_rgba(IconStyle::Tile, 32);
        assert_eq!(t.len(), 32 * 32 * 4);
        assert_eq!(px(&t, 32, 0, 0)[3], 0);
        assert_eq!(px(&t, 32, 16, 3), [0x1F, 0x4E, 0x79, 0xFF]);
        assert_eq!(px(&t, 32, 16, 22), [0xFF, 0xFF, 0xFF, 0xFF]);
    }

    #[test]
    fn mono_white_has_no_background_and_one_colour() {
        let m = icon_rgba(IconStyle::MonoWhite, 16);
        assert_eq!(px(&m, 16, 0, 0)[3], 0);
        assert_eq!(px(&m, 16, 8, 12), [0xFF, 0xFF, 0xFF, 0xFF]);
        assert!(m.chunks(4).all(|p| p[3] == 0 || p[..3] == [0xFF; 3]));
    }

    #[test]
    fn mono_dark_is_near_black() {
        let d = icon_rgba(IconStyle::MonoDark, 16);
        assert_eq!(px(&d, 16, 8, 12), [0x1B, 0x1B, 0x1B, 0xFF]);
    }

    #[test]
    fn cards_are_split_by_a_clear_gap_at_16_px() {
        // Column 9: the front card ends left of it, the middle card resumes right of it.
        let m = icon_rgba(IconStyle::MonoWhite, 16);
        assert_eq!(px(&m, 16, 8, 9)[3], 0xFF);
        assert_eq!(px(&m, 16, 9, 9)[3], 0);
        assert_eq!(px(&m, 16, 10, 9)[3], 0xFF);
        // Row 4: the gap between the back and the middle card.
        assert_eq!(px(&m, 16, 8, 3)[3], 0xFF);
        assert_eq!(px(&m, 16, 8, 4)[3], 0);
        assert_eq!(px(&m, 16, 8, 5)[3], 0xFF);
    }

    #[test]
    fn small_tiles_keep_crisp_gaps() {
        // Opaque white card, then a gap of pure tile colour, then the next card.
        let blue = [0x1F, 0x4E, 0x79, 0xFF];
        let t = icon_rgba(IconStyle::Tile, 16);
        assert_eq!(px(&t, 16, 9, 9), [0xFF; 4]);
        assert_eq!(px(&t, 16, 10, 9), blue);
        assert_eq!(px(&t, 16, 11, 9), [0xFF; 4]);
        let t = icon_rgba(IconStyle::Tile, 24);
        assert_eq!(px(&t, 24, 13, 12), [0xFF; 4]);
        assert_eq!(px(&t, 24, 14, 12), blue);
        assert_eq!(px(&t, 24, 15, 12), [0xFF; 4]);
    }

    #[test]
    fn any_size_is_square_rgba() {
        for size in [16, 24, 48, 256] {
            assert_eq!(
                icon_rgba(IconStyle::Tile, size).len(),
                (size * size * 4) as usize
            );
        }
    }
}
