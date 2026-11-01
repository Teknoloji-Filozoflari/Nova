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
}

impl UiTheme {
    /// Kalıcı ad (state.toml).
    pub fn id(self) -> &'static str {
        match self {
            Self::Default => "varsayilan",
            Self::Light => "acik",
            Self::Forest => "orman",
            Self::Amber => "amber",
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
            _ => Self::Default,
        }
    }

    /// Sonraki tema (döngüsel; geriye dönük / test).
    pub fn next(self) -> Self {
        match self {
            Self::Default => Self::Light,
            Self::Light => Self::Forest,
            Self::Forest => Self::Amber,
            Self::Amber => Self::Default,
        }
    }

    /// Listede seçim için sıra numarası.
    pub fn index(self) -> usize {
        Self::all().iter().position(|t| *t == self).unwrap_or(0)
    }

    /// Tüm temalar (seçici panel / test).
    pub fn all() -> &'static [Self] {
        &[Self::Default, Self::Light, Self::Forest, Self::Amber]
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
        assert_eq!(
            UiTheme::Default.next().next().next().next(),
            UiTheme::Default
        );
        assert_eq!(UiTheme::parse("bilinmeyen"), UiTheme::Default);
    }
}
