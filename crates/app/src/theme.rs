//! Colours and font taken from Windows, so the shelf looks at home there.

use eframe::egui::{self, Color32};
use eframe::epaint::text::{FontInsert, FontPriority, InsertFontFamily};
use shelve_platform::SystemLook;

/// Lowest contrast against the background at which the accent still reads (WCAG, non-text).
const MIN_ACCENT_CONTRAST: f32 = 3.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub bg: Color32,
    pub border: Color32,
    pub text: Color32,
    pub text_weak: Color32,
    pub hover: Color32,
    /// The Windows accent, nudged towards `text` until it stands out from `bg`.
    pub accent: Color32,
    /// Light wash of the accent, for highlighted rows.
    pub accent_bg: Color32,
    pub shadow: Color32,
}

pub fn palette(dark: bool, accent: [u8; 3]) -> Palette {
    let gray = Color32::from_gray;
    let (bg, border, text, text_weak, hover, shadow_alpha) = if dark {
        (
            gray(0x2B),
            gray(0x45),
            Color32::WHITE,
            gray(0xC2),
            gray(0x3A),
            128,
        )
    } else {
        (
            gray(0xF9),
            gray(0xD0),
            gray(0x1B),
            gray(0x5A),
            gray(0xEA),
            46,
        )
    };
    let system = Color32::from_rgb(accent[0], accent[1], accent[2]);
    let accent = (0..=10)
        .map(|step| mix(system, text, step as f32 / 10.0))
        .find(|&c| contrast(c, bg) >= MIN_ACCENT_CONTRAST)
        .unwrap_or(text);
    Palette {
        bg,
        border,
        text,
        text_weak,
        hover,
        accent,
        accent_bg: mix(bg, accent, 0.15),
        shadow: Color32::from_black_alpha(shadow_alpha),
    }
}

/// `t` = 0 gives `a`, 1 gives `b`. Opaque colours only.
fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let ch = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
    Color32::from_rgb(ch(a.r(), b.r()), ch(a.g(), b.g()), ch(a.b(), b.b()))
}

/// WCAG contrast ratio, 1 (same) to 21 (black on white).
pub fn contrast(a: Color32, b: Color32) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

fn luminance(c: Color32) -> f32 {
    let lin = |v: u8| {
        let v = v as f32 / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c.r()) + 0.7152 * lin(c.g()) + 0.0722 * lin(c.b())
}

/// Makes every egui widget, in the shelf and in the settings window, use the palette.
pub fn apply(ctx: &egui::Context, look: SystemLook) -> Palette {
    let p = palette(look.apps_dark, look.accent);
    let mut v = if look.apps_dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    v.panel_fill = p.bg;
    v.window_fill = p.bg;
    v.window_stroke = egui::Stroke::new(1.0, p.border);
    v.window_shadow.color = p.shadow;
    v.popup_shadow.color = p.shadow;
    v.weak_text_color = Some(p.text_weak);
    v.hyperlink_color = p.accent;
    v.selection.bg_fill = p.accent_bg;
    v.selection.stroke.color = p.accent;
    v.widgets.noninteractive.fg_stroke.color = p.text;
    v.widgets.noninteractive.bg_stroke.color = p.border;
    v.widgets.inactive.fg_stroke.color = p.text;
    v.widgets.inactive.weak_bg_fill = p.hover;
    v.widgets.inactive.bg_fill = p.hover;
    v.widgets.hovered.fg_stroke.color = p.text;
    v.widgets.hovered.bg_stroke.color = p.accent;
    v.widgets.active.fg_stroke.color = p.text;
    v.widgets.active.bg_stroke.color = p.accent;
    ctx.set_theme(if look.apps_dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    });
    ctx.set_visuals(v);
    p
}

/// egui animates a few things itself (switch knobs, scrolling to a row). With
/// "Animation effects" off in Windows those must be instant as well.
pub fn set_animations(ctx: &egui::Context, on: bool) {
    let time = if on {
        egui::Style::default().animation_time
    } else {
        0.0
    };
    ctx.all_styles_mut(|style| style.animation_time = time);
}

/// Segoe UI in front of egui's own font. Without the file (not Windows, or a
/// stripped-down install) egui keeps its built-in font.
pub fn install_font(ctx: &egui::Context) {
    let Some(windir) = std::env::var_os("WINDIR") else {
        return;
    };
    let path = std::path::Path::new(&windir).join(r"Fonts\segoeui.ttf");
    let Ok(bytes) = std::fs::read(path) else {
        return;
    };
    ctx.add_font(FontInsert {
        name: "Segoe UI".into(),
        data: egui::FontData::from_owned(bytes),
        families: vec![InsertFontFamily {
            family: egui::FontFamily::Proportional,
            priority: FontPriority::Highest,
        }],
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIN_BLUE: [u8; 3] = [0x0F, 0x6C, 0xBD];

    #[test]
    fn light_palette_uses_spec_colours() {
        let p = palette(false, WIN_BLUE);
        assert_eq!(p.bg, Color32::from_rgb(0xF9, 0xF9, 0xF9));
        assert_eq!(p.text, Color32::from_rgb(0x1B, 0x1B, 0x1B));
    }

    #[test]
    fn dark_palette_uses_spec_colours() {
        let p = palette(true, WIN_BLUE);
        assert_eq!(p.bg, Color32::from_rgb(0x2B, 0x2B, 0x2B));
        assert_eq!(p.text, Color32::WHITE);
    }

    #[test]
    fn contrasting_accent_is_kept_as_is() {
        assert_eq!(
            palette(false, WIN_BLUE).accent,
            Color32::from_rgb(0x0F, 0x6C, 0xBD)
        );
    }

    #[test]
    fn dark_accent_on_dark_theme_is_lightened() {
        let p = palette(true, [0x00, 0x20, 0x60]);
        assert!(contrast(p.accent, p.bg) >= 3.0);
    }

    #[test]
    fn light_accent_on_light_theme_is_darkened() {
        let p = palette(false, [0xFF, 0xF1, 0x00]);
        assert!(contrast(p.accent, p.bg) >= 3.0);
    }

    #[test]
    fn accent_equal_to_background_still_becomes_visible() {
        for dark in [false, true] {
            let bg = palette(dark, WIN_BLUE).bg;
            let p = palette(dark, [bg.r(), bg.g(), bg.b()]);
            assert!(contrast(p.accent, p.bg) >= 3.0);
        }
    }

    #[test]
    fn contrast_follows_wcag() {
        assert!((contrast(Color32::BLACK, Color32::WHITE) - 21.0).abs() < 0.01);
        assert!((contrast(Color32::WHITE, Color32::WHITE) - 1.0).abs() < 0.01);
        // The order of the two colours does not matter.
        assert_eq!(
            contrast(Color32::BLACK, Color32::WHITE),
            contrast(Color32::WHITE, Color32::BLACK)
        );
    }

    #[test]
    fn accent_bg_is_a_light_wash_of_the_accent() {
        let p = palette(false, WIN_BLUE);
        // 15% of #0F6CBD over #F9F9F9.
        assert_eq!(p.accent_bg, Color32::from_rgb(0xD6, 0xE4, 0xF0));
    }

    #[test]
    fn apply_switches_egui_between_light_and_dark() {
        let ctx = egui::Context::default();
        let look = |apps_dark| SystemLook {
            apps_dark,
            taskbar_dark: true,
            accent: WIN_BLUE,
        };
        apply(&ctx, look(true));
        assert!(ctx.global_style().visuals.dark_mode);
        assert_eq!(
            ctx.global_style().visuals.panel_fill,
            Color32::from_rgb(0x2B, 0x2B, 0x2B)
        );
        apply(&ctx, look(false));
        assert!(!ctx.global_style().visuals.dark_mode);
        assert_eq!(
            ctx.global_style().visuals.panel_fill,
            Color32::from_rgb(0xF9, 0xF9, 0xF9)
        );
    }
    #[test]
    fn animations_off_stops_eguis_own_animations_too() {
        // Switch knobs and scrolling are animated by egui, not by our row code.
        let ctx = egui::Context::default();
        set_animations(&ctx, false);
        assert_eq!(ctx.global_style().animation_time, 0.0);
        set_animations(&ctx, true);
        assert!(ctx.global_style().animation_time > 0.0);
    }
}
