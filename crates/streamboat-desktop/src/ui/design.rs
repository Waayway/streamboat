//! Design tokens (D-012, D-013): one streamboat look, expressed as a small
//! struct instead of native-per-OS styling. [`Tokens::dark`] is the default;
//! [`Tokens::light`] is the other owner-required theme. Every screen and
//! custom widget style closure in `ui/` reads colours, radii, spacing and
//! type sizes from a `Tokens` value rather than reaching into `iced::Theme`
//! directly, so this struct stays the single source of truth. Stock iced
//! widgets (button, text_input, slider, scrollable, ...) still get a
//! consistent look "for free" through [`Tokens::iced_theme`], which builds
//! an `iced::Theme::custom` palette from the same tokens — the mapping onto
//! iced's `Theme`/`Style` types D-013 expects.

use iced::Color;
use streamboat_core::config::ThemePreference;

/// The design-token struct. Copy because it is small, read constantly by
/// view code and cheap to recompute per frame from [`ThemePreference`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tokens {
    pub background: Color,
    pub surface: Color,
    pub elevated: Color,
    pub accent: Color,
    pub accent_muted: Color,
    pub text: Color,
    pub muted: Color,
    pub warning: Color,
    pub danger: Color,
    pub success: Color,
    pub border: Color,

    pub radius_sm: f32,
    pub radius_md: f32,
    pub radius_lg: f32,
    pub radius_pill: f32,

    pub space_xs: f32,
    pub space_sm: f32,
    pub space_md: f32,
    pub space_lg: f32,
    pub space_xl: f32,

    pub text_xs: f32,
    pub text_sm: f32,
    pub text_md: f32,
    pub text_lg: f32,
    pub text_xl: f32,
    pub text_display: f32,
}

impl Tokens {
    pub fn for_preference(pref: ThemePreference) -> Self {
        match pref {
            ThemePreference::Dark => Self::dark(),
            ThemePreference::Light => Self::light(),
        }
    }

    pub fn dark() -> Self {
        Self {
            background: Color::from_rgb8(0x0f, 0x10, 0x15),
            surface: Color::from_rgb8(0x17, 0x19, 0x20),
            elevated: Color::from_rgb8(0x20, 0x23, 0x2c),
            accent: Color::from_rgb8(0x6c, 0x8c, 0xff),
            accent_muted: Color::from_rgb8(0x2b, 0x32, 0x4d),
            text: Color::from_rgb8(0xf1, 0xf2, 0xf6),
            muted: Color::from_rgb8(0x93, 0x98, 0xa8),
            warning: Color::from_rgb8(0xe0, 0xaa, 0x3e),
            danger: Color::from_rgb8(0xe5, 0x6b, 0x6b),
            success: Color::from_rgb8(0x59, 0xc9, 0x8b),
            border: Color::from_rgb8(0x2a, 0x2d, 0x37),
            ..Self::scale()
        }
    }

    pub fn light() -> Self {
        Self {
            background: Color::from_rgb8(0xfa, 0xfa, 0xfb),
            surface: Color::from_rgb8(0xff, 0xff, 0xff),
            elevated: Color::from_rgb8(0xf1, 0xf2, 0xf5),
            accent: Color::from_rgb8(0x3f, 0x5c, 0xe0),
            accent_muted: Color::from_rgb8(0xdd, 0xe3, 0xfb),
            text: Color::from_rgb8(0x16, 0x17, 0x1c),
            muted: Color::from_rgb8(0x5b, 0x60, 0x6e),
            warning: Color::from_rgb8(0xa9, 0x74, 0x0a),
            danger: Color::from_rgb8(0xc9, 0x3a, 0x3a),
            success: Color::from_rgb8(0x1f, 0x9d, 0x5a),
            border: Color::from_rgb8(0xe1, 0xe2, 0xe8),
            ..Self::scale()
        }
    }

    /// The radius/spacing/type scale, identical across themes; only colour
    /// changes between dark and light.
    fn scale() -> Self {
        Self {
            background: Color::TRANSPARENT,
            surface: Color::TRANSPARENT,
            elevated: Color::TRANSPARENT,
            accent: Color::TRANSPARENT,
            accent_muted: Color::TRANSPARENT,
            text: Color::TRANSPARENT,
            muted: Color::TRANSPARENT,
            warning: Color::TRANSPARENT,
            danger: Color::TRANSPARENT,
            success: Color::TRANSPARENT,
            border: Color::TRANSPARENT,
            radius_sm: 6.0,
            radius_md: 10.0,
            radius_lg: 16.0,
            radius_pill: 999.0,
            space_xs: 4.0,
            space_sm: 8.0,
            space_md: 16.0,
            space_lg: 24.0,
            space_xl: 40.0,
            text_xs: 12.0,
            text_sm: 14.0,
            text_md: 16.0,
            text_lg: 20.0,
            text_xl: 28.0,
            text_display: 40.0,
        }
    }

    /// A slightly darkened accent for a hovered primary/accent-filled
    /// button — filled buttons have no `elevated`/`surface` background to
    /// swap to on hover the way outline buttons do, so they darken instead.
    pub fn accent_hover(&self) -> Color {
        Color {
            r: self.accent.r * 0.85,
            g: self.accent.g * 0.85,
            b: self.accent.b * 0.85,
            a: self.accent.a,
        }
    }

    /// Builds the `iced::Theme` stock widgets style themselves against.
    pub fn iced_theme(&self, name: &'static str) -> iced::Theme {
        iced::Theme::custom(
            name,
            iced::theme::Palette {
                background: self.background,
                text: self.text,
                primary: self.accent,
                success: self.success,
                warning: self.warning,
                danger: self.danger,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_and_light_are_distinct() {
        assert_ne!(Tokens::dark().background, Tokens::light().background);
        assert_ne!(Tokens::dark().text, Tokens::light().text);
    }

    #[test]
    fn scale_is_shared_across_themes() {
        assert_eq!(Tokens::dark().radius_md, Tokens::light().radius_md);
        assert_eq!(Tokens::dark().space_lg, Tokens::light().space_lg);
    }

    #[test]
    fn for_preference_matches_the_named_constructor() {
        assert_eq!(
            Tokens::for_preference(ThemePreference::Dark).background,
            Tokens::dark().background
        );
        assert_eq!(
            Tokens::for_preference(ThemePreference::Light).background,
            Tokens::light().background
        );
    }
}
