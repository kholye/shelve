//! The Shelve icon as pixels: a shelf with three books, the third one leaning.
//! Pure drawing, no Windows code.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconStyle {
    /// White drawing on a rounded blue tile: Explorer, title bars.
    Tile,
    /// Drawing only, white: for a dark taskbar.
    MonoWhite,
    /// Drawing only, near black: for a light taskbar.
    MonoDark,
}

type Rect = (f32, f32, f32, f32); // x, y, w, h

struct Glyph {
    grid: f32,
    shelf: Rect,
    upright: [Rect; 2],
    leaning: Rect,
    pivot: (f32, f32),
}

/// Drawn for 32 px and up, where there is room for proportions.
const LARGE: Glyph = Glyph {
    grid: 32.0,
    shelf: (4.0, 23.0, 24.0, 3.5),
    upright: [(6.0, 8.0, 5.0, 13.5), (12.5, 11.0, 5.0, 10.5)],
    leaning: (20.5, 8.5, 5.0, 13.5),
    pivot: (23.0, 22.0),
};

/// Drawn pixel by pixel for the 16 px tray slot, so the lean survives.
const SMALL: Glyph = Glyph {
    grid: 16.0,
    shelf: (1.0, 12.0, 14.0, 2.0),
    upright: [(2.0, 4.0, 3.0, 7.0), (6.0, 6.0, 3.0, 5.0)],
    leaning: (11.5, 4.0, 3.0, 7.5),
    pivot: (13.5, 11.5),
};

const LEAN_DEG: f32 = 20.0;
const TILE: [u8; 3] = [0x0F, 0x6C, 0xBD];
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
                        let shrink = |t: f32| (t - 0.5) / TILE_FILL + 0.5;
                        ink += LARGE.covers(shrink(u), shrink(v)) as u32;
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
        if inside(self.shelf, x, y) || self.upright.iter().any(|&r| inside(r, x, y)) {
            return true;
        }
        // Turn the point back instead of turning the book.
        let (sin, cos) = LEAN_DEG.to_radians().sin_cos();
        let (dx, dy) = (x - self.pivot.0, y - self.pivot.1);
        let (bx, by) = (dx * cos - dy * sin, dx * sin + dy * cos);
        inside(self.leaning, bx + self.pivot.0, by + self.pivot.1)
    }
}

fn inside((rx, ry, w, h): Rect, x: f32, y: f32) -> bool {
    (rx..rx + w).contains(&x) && (ry..ry + h).contains(&y)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(rgba: &[u8], size: usize, x: usize, y: usize) -> [u8; 4] {
        let i = (y * size + x) * 4;
        [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
    }

    #[test]
    fn tile_is_a_rounded_blue_square_with_a_white_shelf() {
        let t = icon_rgba(IconStyle::Tile, 32);
        assert_eq!(t.len(), 32 * 32 * 4);
        assert_eq!(px(&t, 32, 0, 0)[3], 0);
        assert_eq!(px(&t, 32, 16, 3), [0x0F, 0x6C, 0xBD, 0xFF]);
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
    fn third_book_leans_towards_the_others() {
        // Above the third book's upright position the lean leaves a gap on the right
        // and covers pixels on the left that an upright book would not reach.
        let m = icon_rgba(IconStyle::MonoWhite, 32);
        assert_eq!(px(&m, 32, 26, 9)[3], 0);
        assert!(px(&m, 32, 19, 12)[3] > 0x80);
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
