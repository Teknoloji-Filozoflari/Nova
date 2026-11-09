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

Yeni uygulama: **Nova-Store** deposuna PR açın; gömülü kopyayı yenilemek için:

```bash
./tools/sync_store_catalog.sh
# PR dalından (birleşmeden önce):
# NOVA_STORE_REF=cursor/catalog-expansion-d86c ./tools/sync_store_catalog.sh
# veya: NOVA_STORE_SRC=/path/to/Nova-Store ./tools/sync_store_catalog.sh
```

Nova-Store genişletme PR’ı açıkken dal adıyla, birleştikten sonra `main`
ile senkronlayıp Nova’da revendor PR açın. Çalışma zamanında imzalı Releases (`store::remote`) ve
isteğe bağlı repo `main` senkronu (`store::repo_sync`) gömülü kopyanın
üzerine güvenli şekilde binebilir; gömülü TOML çevrimdışı yedektir
(`assets/keys/nova-store-catalog.pub`).

## Şema notları

- `supported_platforms`: `linux-x64`, `linux-aarch64`
- `artifacts`: `[[apps.artifacts]]` ile mimariye özel URL/checksum
- `release.repo`: canlı sürüm + detay paneli GitHub yıldızı (doğrulanmış depo)
- `repository_url`: isteğe bağlı `https://github.com/owner/repo` — `release`
  yokken yıldız / Depo linki için; `source` tek başına yıldız için yetmez
- `signature`: isteğe bağlı upstream imza (minisign | cosign | gpg | attestation) — trust class **B**
- Kurulum yöntemleri: `manual`, `download_archive`, `pipx`, `npm`, `cargo`, `brew`, `apt`, `dnf`, `pacman`, `uv`
- Trust class: **A** distro (apt/dnf/pacman), **B** proje ikili+imza, **C** kayıt defteri, **D** manuel

