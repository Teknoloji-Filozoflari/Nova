//! Arayüz temaları.
//!
//! Tema adı `state.toml` `prefs.theme` alanında saklanır. Bilinmeyen adlar
//! varsayılana düşer. Varsayılan palet mockup diline hizalıdır (`#0d1117`
//! zemin, `#00e5ff` cyan vurgu); diğer temalar aynı token yapısını korur.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;

/// Desteklenen arayüz temaları.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UiTheme {
    /// Cyan vurgulu koyu (varsayılan — mockup dili).
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

/// Temanın renk paleti — mockup token merkezi.
#[derive(Debug, Clone, Copy)]
pub struct ThemePalette {
    /// Ana zemin (`#0d1117` varsayılan).
    pub bg: Color,
    /// Pane / kart içi zemin.
    pub bg_panel: Color,
    /// İnce pane/kutu kenarı.
    pub border: Color,
    /// Vurgu (seçim dolgusu, başlık, komut adı, kısayol tuşu).
    pub accent: Color,
    /// Vurgu üzerindeki metin (seçili satır).
    pub accent_on: Color,
    /// Birincil metin.
    pub text: Color,
    /// İkincil / soluk metin.
    pub muted: Color,
    /// Güvenli / başarı (yeşil).
    pub success: Color,
    /// Dikkat / orta / durum sarısı.
    pub warn: Color,
    /// Yıkıcı / hata.
    pub error: Color,
    /// İpucu amber (İPUCU callout).
    pub tip: Color,
    /// Örnek satırında argüman / bayrak rengi.
    pub arg: Color,
    /// Durum satırı (sarı bağlam).
    pub status: Color,
    /// Arama vurgusu.
    pub search: Color,
    /// Kod bloğu sol şerit (genelde accent).
    pub code_border: Color,
    /// Panel/arka plan boyası. `None` = terminal varsayılanı.
    /// Açık temada ve mockup varsayılanında boyanır.
    pub surface: Option<Color>,
}

impl ThemePalette {
    /// Dolu cyan seçim çubuğu (koyu metin).
    pub fn selection(self) -> Style {
        Style::default()
            .fg(self.accent_on)
            .bg(self.accent)
            .add_modifier(Modifier::BOLD)
    }

    /// Seçili ama odaksız satır.
    pub fn selected(self) -> Style {
        Style::default().fg(self.accent)
    }

    /// Normal gövde.
    pub fn body(self) -> Style {
        Style::default().fg(self.text)
    }

    /// Soluk etiket.
    pub fn dim(self) -> Style {
        Style::default().fg(self.muted)
    }

    /// Cyan vurgu metni.
    pub fn accent_fg(self) -> Style {
        Style::default().fg(self.accent)
    }

    /// Odaklı / odaksız kenarlık.
    pub fn border_style(self, focused: bool) -> Style {
        if focused {
            Style::default().fg(self.accent)
        } else {
            Style::default().fg(self.border)
        }
    }

    /// Bölüm başlığı (SÖZDİZİMİ / SEÇENEKLER) — cyan bold.
    pub fn heading(self) -> Style {
        Style::default()
            .fg(self.accent)
            .add_modifier(Modifier::BOLD)
    }

    /// Outlined pill rozeti: `[etiket]` — kenarlık rengi = metin rengi.
    pub fn pill(self, label: &str, color: Color) -> Span<'static> {
        Span::styled(
            format!("[{label}]"),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        )
    }

    /// Öğrenme seviyesi pill rengi.
    pub fn level_color(self, level: crate::commands::LearningLevel) -> Color {
        use crate::commands::LearningLevel;
        match level {
            LearningLevel::Beginner => self.accent,
            LearningLevel::Intermediate => self.warn,
            LearningLevel::Advanced => self.error,
            LearningLevel::Unspecified => self.muted,
        }
    }

    /// Risk pill rengi.
    pub fn risk_color(self, risk: crate::commands::RiskLevel) -> Color {
        use crate::commands::RiskLevel;
        match risk {
            RiskLevel::Safe => self.success,
            RiskLevel::Caution => self.warn,
            RiskLevel::Dangerous | RiskLevel::Destructive => self.error,
        }
    }

    /// Footer kısayol tuşu.
    pub fn key(self) -> Style {
        Style::default()
            .fg(self.accent)
            .add_modifier(Modifier::BOLD)
    }

    /// Footer kısayol etiketi.
    pub fn key_label(self) -> Style {
        Style::default().fg(self.muted)
    }
}

impl UiTheme {
    /// Bu temanın paleti.
    pub fn palette(self) -> ThemePalette {
        match self {
            // Mockup: bg #0d1117 · accent #00e5ff · text #a5b3cf · muted #6272a4
            Self::Default => ThemePalette {
                bg: Color::Rgb(13, 17, 23),
                bg_panel: Color::Rgb(11, 14, 20),
                border: Color::Rgb(30, 38, 51),
                accent: Color::Rgb(0, 229, 255),
                accent_on: Color::Rgb(13, 17, 23),
                text: Color::Rgb(165, 179, 207),
                muted: Color::Rgb(98, 114, 164),
                success: Color::Rgb(61, 214, 140),
                warn: Color::Rgb(240, 180, 41),
                error: Color::Rgb(255, 107, 129),
                tip: Color::Rgb(240, 180, 41),
                arg: Color::Rgb(230, 190, 70),
                status: Color::Rgb(230, 190, 70),
                search: Color::Rgb(240, 180, 41),
                code_border: Color::Rgb(0, 229, 255),
                surface: Some(Color::Rgb(13, 17, 23)),
            },
            Self::Light => ThemePalette {
                bg: Color::Rgb(236, 240, 245),
                bg_panel: Color::Rgb(248, 250, 252),
                border: Color::Rgb(180, 190, 205),
                accent: Color::Rgb(0, 120, 200),
                accent_on: Color::White,
                text: Color::Rgb(20, 28, 40),
                muted: Color::Rgb(90, 100, 120),
                success: Color::Green,
                warn: Color::Rgb(180, 100, 20),
                error: Color::Red,
                tip: Color::Rgb(180, 100, 20),
                arg: Color::Rgb(160, 90, 20),
                status: Color::Rgb(160, 90, 20),
                search: Color::Rgb(0, 120, 200),
                code_border: Color::Rgb(0, 120, 200),
                surface: Some(Color::Rgb(236, 240, 245)),
            },
            Self::Forest => ThemePalette {
                bg: Color::Rgb(12, 18, 14),
                bg_panel: Color::Rgb(10, 16, 12),
                border: Color::Rgb(36, 52, 40),
                accent: Color::Rgb(80, 200, 120),
                accent_on: Color::Rgb(8, 14, 10),
                text: Color::Rgb(210, 230, 215),
                muted: Color::Rgb(110, 140, 120),
                success: Color::LightGreen,
                warn: Color::Yellow,
                error: Color::Red,
                tip: Color::Yellow,
                arg: Color::LightYellow,
                status: Color::Yellow,
                search: Color::LightGreen,
                code_border: Color::Rgb(80, 200, 120),
                surface: Some(Color::Rgb(12, 18, 14)),
            },
            Self::Amber => ThemePalette {
                bg: Color::Rgb(16, 14, 10),
                bg_panel: Color::Rgb(14, 12, 8),
                border: Color::Rgb(48, 40, 28),
                accent: Color::Rgb(255, 180, 40),
                accent_on: Color::Rgb(16, 14, 10),
                text: Color::Rgb(230, 220, 200),
                muted: Color::Rgb(140, 120, 90),
                success: Color::Green,
                warn: Color::LightYellow,
                error: Color::Red,
                tip: Color::LightYellow,
                arg: Color::Yellow,
                status: Color::Yellow,
                search: Color::Yellow,
                code_border: Color::Rgb(255, 180, 40),
                surface: Some(Color::Rgb(16, 14, 10)),
            },
            Self::Ocean => ThemePalette {
                bg: Color::Rgb(10, 16, 24),
                bg_panel: Color::Rgb(8, 14, 22),
                border: Color::Rgb(28, 44, 64),
                accent: Color::Rgb(100, 220, 240),
                accent_on: Color::Rgb(8, 14, 22),
                text: Color::Rgb(200, 220, 235),
                muted: Color::Rgb(100, 130, 160),
                success: Color::LightGreen,
                warn: Color::Yellow,
                error: Color::LightRed,
                tip: Color::Yellow,
                arg: Color::LightYellow,
                status: Color::Yellow,
                search: Color::LightBlue,
                code_border: Color::Rgb(100, 220, 240),
                surface: Some(Color::Rgb(10, 16, 24)),
            },
            Self::Midnight => ThemePalette {
                bg: Color::Rgb(12, 18, 32),
                bg_panel: Color::Rgb(10, 14, 28),
                border: Color::Rgb(32, 42, 64),
                accent: Color::Rgb(120, 170, 255),
                accent_on: Color::Rgb(8, 12, 24),
                text: Color::Rgb(210, 220, 240),
                muted: Color::Rgb(90, 105, 140),
                success: Color::Green,
                warn: Color::Yellow,
                error: Color::Red,
                tip: Color::Yellow,
                arg: Color::LightYellow,
                status: Color::Yellow,
                search: Color::LightBlue,
                code_border: Color::Rgb(120, 170, 255),
                surface: Some(Color::Rgb(12, 18, 32)),
            },
            Self::Rose => ThemePalette {
                bg: Color::Rgb(18, 12, 18),
                bg_panel: Color::Rgb(16, 10, 16),
                border: Color::Rgb(52, 36, 52),
                accent: Color::Rgb(240, 100, 180),
                accent_on: Color::Rgb(14, 8, 14),
                text: Color::Rgb(235, 210, 225),
                muted: Color::Rgb(140, 110, 130),
                success: Color::Green,
                warn: Color::Yellow,
                error: Color::LightRed,
                tip: Color::Yellow,
                arg: Color::LightYellow,
                status: Color::Yellow,
                search: Color::LightMagenta,
                code_border: Color::Rgb(240, 100, 180),
                surface: Some(Color::Rgb(18, 12, 18)),
            },
            Self::Slate => ThemePalette {
                bg: Color::Rgb(28, 32, 38),
                bg_panel: Color::Rgb(24, 28, 34),
                border: Color::Rgb(55, 62, 72),
                accent: Color::Rgb(180, 190, 200),
                accent_on: Color::Rgb(20, 24, 28),
                text: Color::Rgb(230, 235, 240),
                muted: Color::Rgb(120, 128, 140),
                success: Color::LightGreen,
                warn: Color::Yellow,
                error: Color::Red,
                tip: Color::Yellow,
                arg: Color::LightYellow,
                status: Color::Yellow,
                search: Color::White,
                code_border: Color::Rgb(180, 190, 200),
                surface: Some(Color::Rgb(28, 32, 38)),
            },
            Self::Crimson => ThemePalette {
                bg: Color::Rgb(18, 10, 12),
                bg_panel: Color::Rgb(16, 8, 10),
                border: Color::Rgb(52, 28, 32),
                accent: Color::Rgb(255, 100, 110),
                accent_on: Color::Rgb(14, 6, 8),
                text: Color::Rgb(235, 215, 215),
                muted: Color::Rgb(140, 110, 115),
                success: Color::Green,
                warn: Color::Yellow,
                error: Color::Red,
                tip: Color::Yellow,
                arg: Color::LightYellow,
                status: Color::Yellow,
                search: Color::LightRed,
                code_border: Color::Rgb(255, 100, 110),
                surface: Some(Color::Rgb(18, 10, 12)),
            },
        }
    }
}

/// Türkçe duyarklı başlık büyük harfi (mockup: SÖZDİZİMİ).
pub fn heading_caps(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'i' => 'İ',
            'ı' => 'I',
            'ğ' => 'Ğ',
            'ü' => 'Ü',
            'ş' => 'Ş',
            'ö' => 'Ö',
            'ç' => 'Ç',
            other => other.to_uppercase().next().unwrap_or(other),
        })
        .collect()
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

    #[test]
    fn default_palette_matches_mock_tokens() {
        let pal = UiTheme::Default.palette();
        assert_eq!(pal.accent, Color::Rgb(0, 229, 255));
        assert_eq!(pal.bg, Color::Rgb(13, 17, 23));
        assert_eq!(pal.text, Color::Rgb(165, 179, 207));
        assert_eq!(pal.muted, Color::Rgb(98, 114, 164));
        assert!(pal.surface.is_some());
        assert_eq!(heading_caps("sözdizimi"), "SÖZDİZİMİ");
    }
}
