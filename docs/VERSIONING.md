# Sürümleme ve paketleme

Bu belge Nova sürümleme ve dağıtım akışını tanımlar. crates.io / AUR / distro
paketleri hâlâ açık onay ister. **GitHub Releases** ikili yayınları
`.github/workflows/release.yml` ile `v*.*.*` etiketlerinde (veya elle
`gh release`) üretilir; varlık adı
`nova-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz` + `SHA256SUMS.txt` olmalıdır
(Ayarlar→Güncelle / `pick_linux_asset` bunları seçer).

## Sürüm numarası

- Şema: [Semantic Versioning 2.0.0](https://semver.org/) — `MAJOR.MINOR.PATCH`.
- `0.x` dönemi: arayüz ve yapılandırma biçimi değişebilir; `MINOR` artışı
  uyumsuz değişiklik içerebilir. Bunu `CHANGELOG` veya sürüm notunda **açıkça**
  yaz.
- 1.0'dan sonra: katalog alanları (`assets/catalog/*.toml` şeması) ve
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
6. `LICENSE.md` (SPDX seçim özeti), `LICENSE-MIT`, `LICENSE-APACHE` ve
   `Cargo.toml` `license = "MIT OR Apache-2.0"` alanı birbiriyle uyumlu mu?
7. `cargo build --release` ve `./target/release/nova` ile duman testi.
8. Etiket ve yayın yalnızca proje sahibinin onayıyla.

## Derleme ve ikili dosya

- Tek ikili: `nova`. Katalog (`assets/catalog/*.toml`) `include_str!` ile ikiliye
  gömülüdür; çalışma zamanında ayrı veri dosyası gerekmez.
- Çalışma zamanı verisi: `$XDG_CONFIG_HOME/nova/config.toml` (kullanıcı,
  isteğe bağlı) ve `$XDG_CONFIG_HOME/nova/state.toml` (Nova yazar); XDG
  değişkeni tanımlı değilse `~/.config/nova/` kullanılır. Hiçbiri ikili ile
  birlikte dağıtılmaz.
- Gerekli çalışma zamanı bağımlılığı: desteklenen bir terminal backend’i
  (`$TERMINAL` / xdg-terminal-exec / kitty / alacritty / wezterm /
  gnome-terminal / konsole / foot / xterm / tmux); `command_help` türleri
  ayrıca `less` veya `more` ister.
- Hedef platform: Linux x86_64. macOS/Windows desteklenmiyor.

## Paketleme seçenekleri

| Yol | Ne gerekir | Durum |
|-----|-----------|-------|
| Kaynaktan derleme (`cargo build --release`) | Rust araç zinciri | Doğrulandı |
| GitHub release (ikili) | `v*` etiket + `release.yml` veya `gh release`; tar.gz içinde `nova` | `v0.1.0` ile başladı |
| `cargo install --path .` | Rust araç zinciri | Denenmedi |
| crates.io | Lisans metinleri, `license` alanı, yayın onayı | Yapılmadı |
| AUR / distro paketi | Paketçi, lisans, kaynak tarball | Yapılmadı; paket adı yok |

Distro adları (CachyOS, Debian 13, Pardus) yalnızca **hedef** olarak anılır;
burada hiçbir paket adı veya kurulum komutu doğrulanmamıştır.

## Bağımlılık bakımı

- `Cargo.lock` depoda tutulur (ikili uygulama).
- CI (`.github/workflows/ci.yml`): fmt, clippy, test; ayrıca `cargo audit` +
  `cargo deny check` ve haftalık zamanlama. Dependabot Cargo/Actions açar.
- Yeni bağımlılık eklerken: gerçekten gerekli mi, lisansı projenin lisansıyla
  uyumlu mu, MSRV'yi yükseltiyor mu?
