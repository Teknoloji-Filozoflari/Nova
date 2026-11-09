//! Kategori görünen adları (katalog / mağaza / komut bankası).
//!
//! Veri kimlikleri (`CATEGORIES`, `STORE_CATEGORIES`, banka slug'ları) dil
//! değişmez; yalnızca ekranda gösterilen etiketler yerelleştirilir.

use super::Language;
use crate::catalog::{CATEGORIES, SHORT_CATEGORY_NAMES};
use crate::store::STORE_CATEGORIES;

/// 7 dil: Tr, En, Ru, Az, Uz, Kk, Tk.
type L7 = [&'static str; 7];

const fn pick(row: &L7, lang: Language) -> &'static str {
    match lang {
        Language::Tr => row[0],
        Language::En => row[1],
        Language::Ru => row[2],
        Language::Az => row[3],
        Language::Uz => row[4],
        Language::Kk => row[5],
        Language::Tk => row[6],
    }
}

/// Katalog kısa adları — sırası [`CATEGORIES`] / [`SHORT_CATEGORY_NAMES`] ile aynı.
const CATALOG_SHORT: [L7; 8] = [
    ["AI", "AI", "ИИ", "AI", "AI", "AI", "AI"],
    ["Git", "Git", "Git", "Git", "Git", "Git", "Git"],
    ["Dosya", "Files", "Файлы", "Fayl", "Fayl", "Файл", "Faýl"],
    [
        "Sistem",
        "System",
        "Система",
        "Sistem",
        "Tizim",
        "Жүйе",
        "Sistema",
    ],
    ["Ağ", "Net", "Сеть", "Şəbəkə", "Tarmoq", "Желі", "Tor"],
    [
        "Docker", "Docker", "Docker", "Docker", "Docker", "Docker", "Docker",
    ],
    [
        "Yardımcılar",
        "Helpers",
        "Утилиты",
        "Köməkçilər",
        "Yordamchilar",
        "Көмекшілер",
        "Kömekçiler",
    ],
    [
        "Özel",
        "Custom",
        "Свои",
        "Xüsusi",
        "Maxsus",
        "Арнайы",
        "Ýörite",
    ],
];

/// Katalog tam adları — sırası [`CATEGORIES`] ile aynı.
const CATALOG_FULL: [L7; 8] = [
    [
        "AI ve Kodlama",
        "AI & Coding",
        "ИИ и код",
        "AI və Kodlama",
        "AI va Kodlash",
        "AI және код",
        "AI we Kodlama",
    ],
    [
        "Git ve Geliştirici Araçları",
        "Git & Dev Tools",
        "Git и разработка",
        "Git və İnkişaf",
        "Git va Dasturlash",
        "Git және әзірлеу",
        "Git we Ösüş",
    ],
    [
        "Dosya Yönetimi ve Arama",
        "Files & Search",
        "Файлы и поиск",
        "Fayl və Axtarış",
        "Fayl va Qidiruv",
        "Файл және іздеу",
        "Faýl we Gözleg",
    ],
    [
        "Sistem ve Kaynak Kullanımı",
        "System & Resources",
        "Система и ресурсы",
        "Sistem və Resurslar",
        "Tizim va Resurslar",
        "Жүйе және ресурстар",
        "Sistema we Serişdeler",
    ],
    [
        "Ağ ve Bağlantı",
        "Network & Connectivity",
        "Сеть и связь",
        "Şəbəkə və Bağlantı",
        "Tarmoq va Aloqa",
        "Желі және байланыс",
        "Tor we Birikme",
    ],
    [
        "Docker ve Sunucu",
        "Docker & Servers",
        "Docker и серверы",
        "Docker və Server",
        "Docker va Server",
        "Docker және сервер",
        "Docker we Serwer",
    ],
    [
        "Günlük Terminal Yardımcıları",
        "Everyday Terminal Helpers",
        "Повседневные утилиты",
        "Gündəlik Terminal Köməkçiləri",
        "Kundalik Terminal Yordamchilari",
        "Күнделікті терминал көмекшілері",
        "Gündelik Terminal Kömekçileri",
    ],
    [
        "Özel Araçlar",
        "Custom Tools",
        "Свои инструменты",
        "Xüsusi Alətlər",
        "Maxsus Vositalar",
        "Арнайы құралдар",
        "Ýörite Gurallar",
    ],
];

/// Mağaza kategori etiketleri — sırası [`STORE_CATEGORIES`] ile aynı.
const STORE_LABELS: [L7; 18] = [
    ["AI", "AI", "ИИ", "AI", "AI", "AI", "AI"],
    ["Git", "Git", "Git", "Git", "Git", "Git", "Git"],
    [
        "Dosya ve Arama",
        "Files & Search",
        "Файлы и поиск",
        "Fayl və Axtarış",
        "Fayl va Qidiruv",
        "Файл және іздеу",
        "Faýl we Gözleg",
    ],
    [
        "Geliştirme",
        "Development",
        "Разработка",
        "İnkişaf",
        "Dasturlash",
        "Әзірлеу",
        "Ösüş",
    ],
    [
        "Editörler",
        "Editors",
        "Редакторы",
        "Redaktorlar",
        "Muharrirlar",
        "Өңдегіштер",
        "Redaktorlar",
    ],
    [
        "Sistem",
        "System",
        "Система",
        "Sistem",
        "Tizim",
        "Жүйе",
        "Sistema",
    ],
    [
        "Paneller",
        "Dashboards",
        "Панели",
        "Panellər",
        "Panellar",
        "Панельдер",
        "Paneller",
    ],
    ["Ağ", "Network", "Сеть", "Şəbəkə", "Tarmoq", "Желі", "Tor"],
    [
        "Konteyner",
        "Containers",
        "Контейнеры",
        "Konteyner",
        "Konteyner",
        "Контейнер",
        "Konteýner",
    ],
    [
        "Veritabanı",
        "Database",
        "База данных",
        "Verilənlər bazası",
        "Ma'lumotlar bazasi",
        "Дерекқор",
        "Maglumat bazasy",
    ],
    [
        "Güvenlik",
        "Security",
        "Безопасность",
        "Təhlükəsizlik",
        "Xavfsizlik",
        "Қауіпсіздік",
        "Howpsuzlyk",
    ],
    [
        "Medya",
        "Media",
        "Медиа",
        "Media",
        "Media",
        "Медиа",
        "Media",
    ],
    [
        "Mesajlaşma",
        "Messaging",
        "Сообщения",
        "Mesajlaşma",
        "Xabarlashuv",
        "Хабарлама",
        "Habarlaşyk",
    ],
    [
        "Verimlilik",
        "Productivity",
        "Продуктивность",
        "Məhsuldarlıq",
        "Samaradorlik",
        "Өнімділік",
        "Önümçilik",
    ],
    [
        "Oyunlar",
        "Games",
        "Игры",
        "Oyunlar",
        "O'yinlar",
        "Ойындар",
        "Oýunlar",
    ],
    [
        "Ekran Koruyucular",
        "Screensavers",
        "Заставки",
        "Ekran qoruyucuları",
        "Ekran saqlovchilar",
        "Экран сақтағыштар",
        "Ekran goraýjylar",
    ],
    ["Web", "Web", "Веб", "Veb", "Veb", "Веб", "Web"],
    [
        "Günlük",
        "Everyday",
        "Повседневное",
        "Gündəlik",
        "Kundalik",
        "Күнделікті",
        "Gündelik",
    ],
];

/// Banka kategori slug → 7 dil etiketi.
const BANK_LABELS: &[(&str, L7)] = &[
    (
        "archives",
        [
            "Arşivler",
            "Archives",
            "Архивы",
            "Arxivlər",
            "Arxivlar",
            "Мұрағаттар",
            "Arhiwler",
        ],
    ),
    (
        "backup",
        [
            "Yedekleme",
            "Backup",
            "Резервное копирование",
            "Ehtiyat nüsxə",
            "Zaxira",
            "Сақтық көшірме",
            "Ätiýaçlyk",
        ],
    ),
    (
        "cloud",
        [
            "Bulut",
            "Cloud",
            "Облако",
            "Bulud",
            "Bulut",
            "Бұлт",
            "Bulut",
        ],
    ),
    (
        "containers",
        [
            "Konteynerler",
            "Containers",
            "Контейнеры",
            "Konteynerlər",
            "Konteynerlar",
            "Контейнерлер",
            "Konteýnerler",
        ],
    ),
    (
        "databases",
        [
            "Veritabanları",
            "Databases",
            "Базы данных",
            "Verilənlər bazaları",
            "Ma'lumotlar bazalari",
            "Дерекқорлар",
            "Maglumat bazalary",
        ],
    ),
    (
        "development",
        [
            "Geliştirme",
            "Development",
            "Разработка",
            "İnkişaf",
            "Dasturlash",
            "Әзірлеу",
            "Ösüş",
        ],
    ),
    (
        "devops",
        [
            "DevOps", "DevOps", "DevOps", "DevOps", "DevOps", "DevOps", "DevOps",
        ],
    ),
    (
        "disks",
        [
            "Diskler",
            "Disks",
            "Диски",
            "Disklər",
            "Disklar",
            "Дискілер",
            "Diskler",
        ],
    ),
    (
        "filesystem",
        [
            "Dosya sistemi",
            "Filesystem",
            "Файловая система",
            "Fayl sistemi",
            "Fayl tizimi",
            "Файлдық жүйе",
            "Faýl ulgamy",
        ],
    ),
    (
        "hardware",
        [
            "Donanım",
            "Hardware",
            "Оборудование",
            "Aparat",
            "Apparat",
            "Жабдық",
            "Enjam",
        ],
    ),
    (
        "monitoring",
        [
            "İzleme",
            "Monitoring",
            "Мониторинг",
            "Monitorinq",
            "Monitoring",
            "Бақылау",
            "Gözegçilik",
        ],
    ),
    (
        "network",
        ["Ağ", "Network", "Сеть", "Şəbəkə", "Tarmoq", "Желі", "Tor"],
    ),
    (
        "packages",
        [
            "Paketler",
            "Packages",
            "Пакеты",
            "Paketlər",
            "Paketlar",
            "Пакеттер",
            "Paketler",
        ],
    ),
    (
        "processes",
        [
            "Süreçler",
            "Processes",
            "Процессы",
            "Proseslər",
            "Jarayonlar",
            "Үдерістер",
            "Prosesler",
        ],
    ),
    (
        "search",
        [
            "Arama",
            "Search",
            "Поиск",
            "Axtarış",
            "Qidiruv",
            "Іздеу",
            "Gözleg",
        ],
    ),
    (
        "security",
        [
            "Güvenlik",
            "Security",
            "Безопасность",
            "Təhlükəsizlik",
            "Xavfsizlik",
            "Қауіпсіздік",
            "Howpsuzlyk",
        ],
    ),
    (
        "shell",
        [
            "Kabuk",
            "Shell",
            "Оболочка",
            "Qabıq",
            "Shell",
            "Қабықша",
            "Gapak",
        ],
    ),
    (
        "system",
        [
            "Sistem",
            "System",
            "Система",
            "Sistem",
            "Tizim",
            "Жүйе",
            "Sistema",
        ],
    ),
    (
        "text",
        ["Metin", "Text", "Текст", "Mətn", "Matn", "Мәтін", "Tekst"],
    ),
    (
        "users",
        [
            "Kullanıcılar",
            "Users",
            "Пользователи",
            "İstifadəçilər",
            "Foydalanuvchilar",
            "Пайдаланушылар",
            "Ulanyjylar",
        ],
    ),
    (
        "virtualization",
        [
            "Sanallaştırma",
            "Virtualization",
            "Виртуализация",
            "Virtualizasiya",
            "Virtuallashtirish",
            "Виртуалдау",
            "Wirtualizasiýa",
        ],
    ),
];

impl Language {
    /// Katalog kısa kategori etiketi (sol sekme). Bilinmeyen → kimlik.
    pub fn catalog_short(self, full_id: &str) -> &str {
        if let Some(i) = CATEGORIES.iter().position(|c| *c == full_id) {
            return pick(&CATALOG_SHORT[i], self);
        }
        SHORT_CATEGORY_NAMES
            .iter()
            .position(|s| *s == full_id)
            .map(|i| pick(&CATALOG_SHORT[i], self))
            .unwrap_or(full_id)
    }

    /// Katalog tam kategori etiketi (liste başlığı / detay). Bilinmeyen → kimlik.
    pub fn catalog_category(self, full_id: &str) -> &str {
        CATEGORIES
            .iter()
            .position(|c| *c == full_id)
            .map(|i| pick(&CATALOG_FULL[i], self))
            .unwrap_or(full_id)
    }

    /// Mağaza kategori etiketi. Bilinmeyen → kimlik.
    pub fn store_category(self, id: &str) -> &str {
        STORE_CATEGORIES
            .iter()
            .position(|c| *c == id)
            .map(|i| pick(&STORE_LABELS[i], self))
            .unwrap_or(id)
    }

    /// Komut bankası kategori slug etiketi. Bilinmeyen → slug.
    pub fn bank_category(self, slug: &str) -> &str {
        BANK_LABELS
            .iter()
            .find(|(s, _)| *s == slug)
            .map(|(_, row)| pick(row, self))
            .unwrap_or(slug)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_short_covers_every_category_and_language() {
        assert_eq!(CATALOG_SHORT.len(), CATEGORIES.len());
        assert_eq!(CATALOG_FULL.len(), CATEGORIES.len());
        assert_eq!(STORE_LABELS.len(), STORE_CATEGORIES.len());
        for lang in Language::all() {
            for cat in CATEGORIES {
                let short = lang.catalog_short(cat);
                let full = lang.catalog_category(cat);
                assert!(!short.is_empty(), "{lang:?} {cat}");
                assert!(!full.is_empty(), "{lang:?} {cat}");
            }
            for cat in STORE_CATEGORIES {
                assert!(!lang.store_category(cat).is_empty(), "{lang:?} {cat}");
            }
            assert!(!lang.bank_category("network").is_empty());
        }
        // TR kısa adlar mevcut sabitlerle uyumlu.
        assert_eq!(
            Language::Tr.catalog_short("AI ve Kodlama"),
            SHORT_CATEGORY_NAMES[0]
        );
        assert_eq!(
            Language::En.catalog_short("Dosya Yönetimi ve Arama"),
            "Files"
        );
        assert_eq!(Language::En.store_category("Geliştirme"), "Development");
        assert_eq!(Language::Ru.bank_category("packages"), "Пакеты");
        assert_eq!(Language::Az.bank_category("missing-slug"), "missing-slug");
    }
}
