# Sürümleme ve paketleme

Bu belge Nova sürümleme ve dağıtım akışını tanımlar. crates.io / AUR hâlâ açık
onay ister. **GitHub Releases** ikili ve distro paketleri
`.github/workflows/release.yml` ile `v*.*.*` etiketlerinde (veya
`workflow_dispatch` + tag) üretilir.

Self-update (`Ayarlar → Güncelle` / `U`) varlıkları
`update::pick_release_asset*` ile host OS/arch’a göre seçer. Linux x86_64
için `pick_linux_asset*` geriye dönük olarak aynı davranışı korur.

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
9. Etiket itildiğinde `release.yml` matrix paketleri + `SHA256SUMS.txt`
   üretir. Tag **itilmezse** Release oluşmaz; `workflow_dispatch` ile tag
   verilerek de yayınlanabilir. Bu repo otomatik tag oluşturmaz.

## Derleme ve ikili dosya

- Tek ikili: `nova` (`nova.exe` on Windows). Katalog (`assets/catalog/*.toml`)
  `include_str!` ile ikiliye gömülüdür; çalışma zamanında ayrı veri dosyası
  gerekmez.
- Çalışma zamanı verisi: `$XDG_CONFIG_HOME/nova/config.toml` (kullanıcı,
  isteğe bağlı) ve `$XDG_CONFIG_HOME/nova/state.toml` (Nova yazar); XDG
  değişkeni tanımlı değilse `~/.config/nova/` kullanılır. Hiçbiri ikili ile
  birlikte dağıtılmaz.
- Gerekli çalışma zamanı bağımlılığı: desteklenen bir terminal backend’i
  (`$TERMINAL` / xdg-terminal-exec / kitty / alacritty / wezterm /
  gnome-terminal / konsole / foot / xterm / tmux); `command_help` türleri
  ayrıca `less` veya `more` ister.
- Hedef platformlar: Linux x86_64 + aarch64 (birincil); Windows x86_64 ve
  macOS x86_64/aarch64 paketleri yayımlanır (Linux-first TUI; bazı mağaza
  yolları Linux paket yöneticilerine bağlıdır).

## Paketleme seçenekleri

| Yol | Ne gerekir | Durum |
|-----|-----------|-------|
| Kaynaktan derleme (`cargo build --release`) | Rust araç zinciri | Doğrulandı |
| GitHub release (tar.gz / zip) | `v*` etiket + `release.yml`; arşivde `nova` / `nova.exe` | `v0.2.0` matrix |
| Debian / Ubuntu (`.deb`) | `release.yml` + nfpm (`packaging/`) | `v0.2.0` (amd64 + arm64) |
| Fedora / RHEL-like (`.rpm`) | `release.yml` + nfpm | `v0.2.0` (x86_64 + aarch64) |
| NixOS (`packaging/nix`) | Nix flake / `default.nix` | Expression shipped; `nix run` README’de |
| Windows zip | `windows-latest` MSVC | `v0.2.0` |
| macOS tar.gz | `macos-13` / `macos-14` | `v0.2.0` |
| `cargo install --path .` | Rust araç zinciri | Denenmedi |
| crates.io | Lisans metinleri, `license` alanı, yayın onayı | Yapılmadı |
| AUR | Paketçi, lisans, kaynak tarball | Yapılmadı; paket adı yok |

Release varlık adları (ör. `v0.2.0`):

- `nova-v0.2.0-x86_64-unknown-linux-gnu.tar.gz` / `.deb` / `.rpm`
- `nova-v0.2.0-aarch64-unknown-linux-gnu.tar.gz` / `.deb` / `.rpm`
- `nova-v0.2.0-x86_64-pc-windows-msvc.zip`
- `nova-v0.2.0-x86_64-apple-darwin.tar.gz`
- `nova-v0.2.0-aarch64-apple-darwin.tar.gz`
- `nova-v0.2.0-nix-expr.tar.gz`
- `SHA256SUMS.txt`

İsteğe bağlı imza: `MINISIGN_PRIVATE_KEY` (+ `MINISIGN_PASSWORD`) secret’ları
yoksa paketler imzasız yayımlanır; checksum her zaman eklenir. Ayrıntı:
`packaging/README.md`.

Distro adları (CachyOS, Debian, Pardus, Fedora) hedef olarak anılır; Arch
üzerinde çalışma zamanı tercihi hâlâ **pacman**’dır.

## Bağımlılık bakımı

- `Cargo.lock` depoda tutulur (ikili uygulama).
- CI (`.github/workflows/ci.yml`): fmt, clippy, test; ayrıca `cargo audit` +
  `cargo deny check` ve haftalık zamanlama. Dependabot Cargo/Actions açar.
- Yeni bağımlılık eklerken: gerçekten gerekli mi, lisansı projenin lisansıyla
  uyumlu mu, MSRV'yi yükseltiyor mu?
