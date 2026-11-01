//! Arayüz temaları.
//!
//! Tema adı `state.toml` `prefs.theme` alanında saklanır. Bilinmeyen adlar
//! varsayılana düşer. Renkler terminal paletine yazılır; arka plan her
//! emülatörde boyanmayabilir ama vurgu/metin renkleri değişir.

use ratatui::style::Color;

/// Desteklenen arayüz temaları.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UiTheme {
    /// Cyan vurgulu koyu (varsayılan).
    #[default]
    Default,
    /// Açık/yüksek kontrast.
    Light,
    /// Yeşil vurgu.
    Forest,
    /// Amber/turuncu vurgu.
    Amber,
    /// Okyanus mavisi vurgu.
    Ocean,
    /// Gece / derin mavi.
    Midnight,
    /// Gül / magenta vurgu.
    Rose,
    /// Arduvaz / soğuk gri.
    Slate,
    /// Kızıl vurgu.
    Crimson,
}

impl UiTheme {
    /// Kalıcı ad (state.toml).
    pub fn id(self) -> &'static str {
        match self {
            Self::Default => "varsayilan",
            Self::Light => "acik",
            Self::Forest => "orman",
            Self::Amber => "amber",
            Self::Ocean => "okyanus",
            Self::Midnight => "gece",
            Self::Rose => "gul",
            Self::Slate => "arduvaz",
            Self::Crimson => "kizil",
        }
    }

    /// Türkçe etiket (geriye dönük).
    pub fn label(self) -> &'static str {
        self.label_for(crate::i18n::Language::Tr)
    }

    /// Dile göre tema etiketi.
    pub fn label_for(self, lang: crate::i18n::Language) -> &'static str {
        use crate::i18n::Msg;
        match self {
            Self::Default => lang.t(Msg::ThemeDefault),
            Self::Light => lang.t(Msg::ThemeLight),
            Self::Forest => lang.t(Msg::ThemeForest),
            Self::Amber => lang.t(Msg::ThemeAmber),
            Self::Ocean => lang.t(Msg::ThemeOcean),
            Self::Midnight => lang.t(Msg::ThemeMidnight),
            Self::Rose => lang.t(Msg::ThemeRose),
            Self::Slate => lang.t(Msg::ThemeSlate),
            Self::Crimson => lang.t(Msg::ThemeCrimson),
        }
    }

    /// Kayıtlı addan tema; bilinmeyen → varsayılan.
    pub fn parse(name: &str) -> Self {
        match name.trim().to_lowercase().as_str() {
            "" | "varsayilan" | "varsayılan" | "default" | "dark" | "karanlik" | "karanlık" => {
                Self::Default
            }
            "acik" | "açık" | "light" => Self::Light,
            "orman" | "forest" | "yesil" | "yeşil" => Self::Forest,
            "amber" | "turuncu" => Self::Amber,
            "okyanus" | "ocean" | "deniz" => Self::Ocean,
            "gece" | "midnight" | "night" => Self::Midnight,
            "gul" | "gül" | "rose" | "magenta" => Self::Rose,
            "arduvaz" | "slate" | "gri" => Self::Slate,
            "kizil" | "kızıl" | "crimson" | "red" => Self::Crimson,
            _ => Self::Default,
        }
    }

    /// Sonraki tema (döngüsel; geriye dönük / test).
    pub fn next(self) -> Self {
        let all = Self::all();
        let i = self.index();
        all[(i + 1) % all.len()]
    }

    /// Listede seçim için sıra numarası.
    pub fn index(self) -> usize {
        Self::all().iter().position(|t| *t == self).unwrap_or(0)
    }

    /// Tüm temalar (seçici panel / test).
    pub fn all() -> &'static [Self] {
        &[
            Self::Default,
            Self::Light,
            Self::Forest,
            Self::Amber,
            Self::Ocean,
            Self::Midnight,
            Self::Rose,
            Self::Slate,
            Self::Crimson,
        ]
    }
}

/// Temanın renk paleti.
#[derive(Debug, Clone, Copy)]
pub struct ThemePalette {
    /// Vurgu (seçim, başlık, kenarlık).
    pub accent: Color,
    /// Vurgu üzerindeki metin (seçili satır).
    pub accent_on: Color,
    /// Birincil metin.
    pub text: Color,
    /// İkincil / soluk metin.
    pub muted: Color,
    /// Başarı.
    pub success: Color,
    /// Uyarı.
    pub warn: Color,
    /// Hata.
    pub error: Color,
    /// Arama vurgusu.
    pub search: Color,
    /// Panel/arka plan boyası. `None` = terminal varsayılanı (koyu temalar).
    /// Açık temada boyanır; aksi hâlde siyah metin koyu emülatörde kaybolur.
    pub surface: Option<Color>,
}

impl UiTheme {
    /// Bu temanın paleti.
    pub fn palette(self) -> ThemePalette {
        match self {
            Self::Default => ThemePalette {
                accent: Color::Cyan,
                accent_on: Color::Black,
                text: Color::White,
                muted: Color::Gray,
                success: Color::Green,
                warn: Color::Yellow,
                error: Color::Red,
                search: Color::Yellow,
                surface: None,
            },
            // Açık tema: yüzey boyanır; metin koyu kalır. Yüzey yoksa bile
            // kategori etiketleri için yeterince kontrastlı muted kullanılır.
            Self::Light => ThemePalette {
                accent: Color::Blue,
                accent_on: Color::White,
                text: Color::Black,
                muted: Color::DarkGray,
                success: Color::Green,
                warn: Color::Magenta,
                error: Color::Red,
                search: Color::Blue,
                surface: Some(Color::Rgb(236, 240, 245)),
            },
            Self::Forest => ThemePalette {
                accent: Color::Green,
                accent_on: Color::Black,
                text: Color::White,
                muted: Color::Gray,
                success: Color::LightGreen,
                warn: Color::Yellow,
                error: Color::Red,
                search: Color::LightGreen,
                surface: None,
            },
            Self::Amber => ThemePalette {
                accent: Color::Yellow,
                accent_on: Color::Black,
                text: Color::White,
                muted: Color::Gray,
                success: Color::Green,
                warn: Color::LightYellow,
                error: Color::Red,
                search: Color::Yellow,
                surface: None,
            },
            Self::Ocean => ThemePalette {
                accent: Color::LightCyan,
                accent_on: Color::Black,
                text: Color::White,
                muted: Color::Gray,
                success: Color::LightGreen,
                warn: Color::Yellow,
                error: Color::LightRed,
                search: Color::LightBlue,
                surface: None,
            },
            Self::Midnight => ThemePalette {
                accent: Color::LightBlue,
                accent_on: Color::Black,
                text: Color::White,
                muted: Color::DarkGray,
                success: Color::Green,
                warn: Color::Yellow,
                error: Color::Red,
                search: Color::LightBlue,
                surface: Some(Color::Rgb(12, 18, 32)),
            },
            Self::Rose => ThemePalette {
                accent: Color::Magenta,
                accent_on: Color::Black,
                text: Color::White,
                muted: Color::Gray,
                success: Color::Green,
                warn: Color::Yellow,
                error: Color::LightRed,
                search: Color::LightMagenta,
                surface: None,
            },
            Self::Slate => ThemePalette {
                accent: Color::Gray,
                accent_on: Color::Black,
                text: Color::White,
                muted: Color::DarkGray,
                success: Color::LightGreen,
                warn: Color::Yellow,
                error: Color::Red,
                search: Color::White,
                surface: Some(Color::Rgb(28, 32, 38)),
            },
            Self::Crimson => ThemePalette {
                accent: Color::LightRed,
                accent_on: Color::Black,
                text: Color::White,
                muted: Color::Gray,
                success: Color::Green,
                warn: Color::Yellow,
                error: Color::Red,
                search: Color::LightRed,
                surface: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_round_trips_and_cycles() {
        for theme in UiTheme::all() {
            assert_eq!(UiTheme::parse(theme.id()), *theme);
            assert!(!theme.label().is_empty());
        }
        let mut theme = UiTheme::Default;
        for _ in 0..UiTheme::all().len() {
            theme = theme.next();
        }
        assert_eq!(theme, UiTheme::Default);
        assert_eq!(UiTheme::parse("bilinmeyen"), UiTheme::Default);
        assert_eq!(UiTheme::parse("okyanus"), UiTheme::Ocean);
        assert_eq!(UiTheme::parse("gece"), UiTheme::Midnight);
    }
}
