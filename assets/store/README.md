# Mağaza kataloğu (gömülü)

Kurulabilir uygulamalar kategori dosyalarına bölünmüştür. Derleme sırasında
`src/store/loader.rs` bu dizindeki `*.toml` dosyalarını `include_str!` ile
okuyup birleştirir.

| Dosya | Kategori (`category`) |
|-------|------------------------|
| `ai.toml` | AI |
| `git.toml` | Git |
| `files.toml` | Dosya ve Arama |
| `dev.toml` | Geliştirme |
| `editors.toml` | Editörler |
| `system.toml` | Sistem |
| `panels.toml` | Paneller |
| `network.toml` | Ağ |
| `container.toml` | Konteyner |
| `database.toml` | Veritabanı |
| `security.toml` | Güvenlik |
| `media.toml` | Medya |
| `messaging.toml` | Mesajlaşma |
| `productivity.toml` | Verimlilik |
| `games.toml` | Oyunlar |
| `screensavers.toml` | Ekran Koruyucular |
| `web.toml` | Web |
| `daily.toml` | Günlük |

Yeni uygulama: doğru kategori dosyasına `[[apps]]` ekleyip yeniden derleyin.

## Şema notları

- `supported_platforms`: `linux-x64`, `linux-aarch64`
- `artifacts`: `[[apps.artifacts]]` ile mimariye özel URL/checksum
- `signature`: isteğe bağlı upstream imza (minisign | cosign | gpg | attestation)
- Kurulum yöntemleri: `manual`, `download_archive`, `pipx`, `npm`, `cargo`, `brew`, `apt`, `dnf`, `pacman`, `uv`

