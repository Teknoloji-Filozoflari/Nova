# Sürümleme ve paketleme (öneri)

Bu belge Nova için **önerilen** sürümleme ve dağıtım akışını tanımlar. Hiçbir
adım otomatik olarak yapılmaz ve burada yazılan hiçbir komut yayın adı
doğrulamaz; yayın (crates.io, AUR, distro paketleri, GitHub release) açık
onay olmadan yapılmamalıdır.

## Sürüm numarası

- Şema: [Semantic Versioning 2.0.0](https://semver.org/) — `MAJOR.MINOR.PATCH`.
- `0.x` dönemi: arayüz ve yapılandırma biçimi değişebilir; `MINOR` artışı
  uyumsuz değişiklik içerebilir. Bunu `CHANGELOG` veya sürüm notunda **açıkça**
  yaz.
- 1.0'dan sonra: katalog alanları (`assets/catalog.toml` şeması) ve
  `$XDG_CONFIG_HOME/nova/config.toml` / `state.toml` (XDG yoksa
  `~/.config/nova/`) biçimi genel API sayılır;
  uyumsuz değişiklik `MAJOR` ister veya eski dosyayı okuyan bir geçiş
  katmanıyla gelir.
- Tek kaynak: `Cargo.toml` içindeki `version`. Etiket (`v0.2.0`) ve changelog
  başlığı aynı olmalı.

## Sürüm çıkarma kontrol listesi (yayın onayı ayrıca gerekir)

1. `cargo fmt --check`
2. `cargo clippy --all-targets --all-features -- -D warnings`
3. `cargo test`
4. Gerçek ortamda Konsole senaryoları (README "Doğrulanmış / doğrulanmamış"
   tablosundaki satırlar) yeniden kontrol edilir.
5. `Cargo.toml` sürümü ve `CHANGELOG.md` güncellenir.
6. `LICENSE-MIT` ve `LICENSE-APACHE` dosyaları ile `Cargo.toml` içindeki
   `MIT OR Apache-2.0` metadata'sı birbiriyle uyumlu mu?
7. `cargo build --release` ve `./target/release/nova` ile duman testi.
8. Etiket ve yayın yalnızca proje sahibinin onayıyla.

## Derleme ve ikili dosya

- Tek ikili: `nova`. Katalog (`assets/catalog.toml`) `include_str!` ile ikiliye
  gömülüdür; çalışma zamanında ayrı veri dosyası gerekmez.
- Çalışma zamanı verisi: `$XDG_CONFIG_HOME/nova/config.toml` (kullanıcı,
  isteğe bağlı) ve `$XDG_CONFIG_HOME/nova/state.toml` (Nova yazar); XDG
  değişkeni tanımlı değilse `~/.config/nova/` kullanılır. Hiçbiri ikili ile
  birlikte dağıtılmaz.
- Gerekli çalışma zamanı bağımlılığı: `konsole` (yalnızca yeni sekmede
  başlatma için; `command_help` türleri ayrıca `less` veya `more` ister).
- Hedef platform: Linux. macOS/Windows desteklenmiyor (denenmedi ve
  Konsole adapteri yok).

## Paketleme seçenekleri (hiçbiri doğrulanmadı)

| Yol | Ne gerekir | Durum |
|-----|-----------|-------|
| Kaynaktan derleme (`cargo build --release`) | Rust araç zinciri | Bu depoda doğrulanan tek yol |
| `cargo install --path .` | Rust araç zinciri | Denenmedi |
| crates.io | Lisans metinleri, `license` alanı, yayın onayı | Yapılmadı; yayın onayı bekliyor |
| AUR / distro paketi | Paketçi, lisans, kaynak tarball | Yapılmadı; paket adı yok |
| GitHub release (ikili) | Etiket, imza/checksum, lisans | Yapılmadı |

Distro adları (CachyOS, Debian 13, Pardus) yalnızca **hedef** olarak anılır;
burada hiçbir paket adı veya kurulum komutu doğrulanmamıştır.

## Bağımlılık bakımı

- `Cargo.lock` depoda tutulur (ikili uygulama).
- Güvenlik denetimi (`cargo-audit` gibi araçlar) bu ortamda kurulu değildi ve
  **çalıştırılmadı**; CI'ya eklenmesi önerilir. `.github/workflows/ci.yml`
  yalnızca fmt/clippy/test çalıştırır ve o dosya da GitHub'a itilmediği için
  hiç çalıştırılmamıştır.
- Yeni bağımlılık eklerken: gerçekten gerekli mi, lisansı projenin lisansıyla
  uyumlu mu, MSRV'yi yükseltiyor mu?
