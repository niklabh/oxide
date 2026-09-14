//! Chrome colour palettes for the Oxide desktop shell.
//!
//! The active palette is installed once per frame via [`install`] and read by
//! the `theme::*` helpers in `ui.rs`. Guest widgets and the browser chrome
//! share the same colours so a light/dark switch is consistent.

use std::cell::Cell;

use crate::prefs::ThemePreference;

/// RGB palette used by the GPUI chrome and host-drawn widgets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub bg: u32,
    pub surface: u32,
    pub surface_hover: u32,
    pub muted: u32,
    pub border: u32,
    pub border_strong: u32,
    pub ring: u32,
    pub primary: u32,
    pub primary_fg: u32,
    pub primary_hover: u32,
    pub fg: u32,
    pub fg_muted: u32,
    pub fg_dim: u32,
    pub accent: u32,
    pub destructive: u32,
    pub destructive_hover: u32,
    pub success: u32,
    pub selection: u32,
}

impl Palette {
    /// Zinc-based dark palette (the historic Oxide chrome).
    pub const DARK: Self = Self {
        bg: 0x0a0a0b,
        surface: 0x18181b,
        surface_hover: 0x27272a,
        muted: 0x27272a,
        border: 0x27272a,
        border_strong: 0x3f3f46,
        ring: 0xd4d4d8,
        primary: 0xfafafa,
        primary_fg: 0x18181b,
        primary_hover: 0xe4e4e7,
        fg: 0xfafafa,
        fg_muted: 0xa1a1aa,
        fg_dim: 0x71717a,
        accent: 0x60a5fa,
        destructive: 0x7f1d1d,
        destructive_hover: 0x991b1b,
        success: 0x16a34a,
        selection: 0x60a5fa55,
    };

    /// Zinc-based light palette.
    pub const LIGHT: Self = Self {
        bg: 0xf4f4f5,
        surface: 0xffffff,
        surface_hover: 0xf4f4f5,
        muted: 0xe4e4e7,
        border: 0xe4e4e7,
        border_strong: 0xd4d4d8,
        ring: 0x18181b,
        primary: 0x18181b,
        primary_fg: 0xfafafa,
        primary_hover: 0x27272a,
        fg: 0x18181b,
        fg_muted: 0x52525b,
        fg_dim: 0xa1a1aa,
        accent: 0x2563eb,
        destructive: 0xdc2626,
        destructive_hover: 0xb91c1c,
        success: 0x16a34a,
        selection: 0x2563eb55,
    };

    pub fn resolve(pref: ThemePreference) -> Self {
        if pref.is_dark() {
            Self::DARK
        } else {
            Self::LIGHT
        }
    }
}

thread_local! {
    static CURRENT: Cell<Palette> = const { Cell::new(Palette::DARK) };
}

/// Install `palette` for the rest of this thread's render pass.
pub fn install(palette: Palette) {
    CURRENT.set(palette);
}

/// The palette last passed to [`install`] on this thread.
pub fn current() -> Palette {
    CURRENT.get()
}

pub fn bg() -> u32 {
    current().bg
}
pub fn surface() -> u32 {
    current().surface
}
pub fn surface_hover() -> u32 {
    current().surface_hover
}
pub fn muted() -> u32 {
    current().muted
}
pub fn border() -> u32 {
    current().border
}
pub fn border_strong() -> u32 {
    current().border_strong
}
pub fn ring() -> u32 {
    current().ring
}
pub fn primary() -> u32 {
    current().primary
}
pub fn primary_fg() -> u32 {
    current().primary_fg
}
pub fn primary_hover() -> u32 {
    current().primary_hover
}
pub fn fg() -> u32 {
    current().fg
}
pub fn fg_muted() -> u32 {
    current().fg_muted
}
pub fn fg_dim() -> u32 {
    current().fg_dim
}
pub fn accent() -> u32 {
    current().accent
}
pub fn destructive() -> u32 {
    current().destructive
}
pub fn destructive_hover() -> u32 {
    current().destructive_hover
}
pub fn success() -> u32 {
    current().success
}

/// Translucent selection highlight.
pub fn selection() -> gpui::Rgba {
    gpui::rgba(current().selection)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_honours_explicit_preference() {
        assert_eq!(Palette::resolve(ThemePreference::Dark), Palette::DARK);
        assert_eq!(Palette::resolve(ThemePreference::Light), Palette::LIGHT);
    }

    #[test]
    fn install_updates_thread_local_helpers() {
        install(Palette::LIGHT);
        assert_eq!(bg(), Palette::LIGHT.bg);
        assert_eq!(fg(), Palette::LIGHT.fg);
        install(Palette::DARK);
        assert_eq!(bg(), Palette::DARK.bg);
    }
}
