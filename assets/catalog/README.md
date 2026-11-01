# Araç kataloğu (gömülü)

Varsayılan araçlar kategori dosyalarına bölünmüştür. Derleme sırasında
`src/catalog/loader.rs` bu dizindeki `*.toml` dosyalarını `include_str!` ile
okuyup birleştirir.

| Dosya | Kategori (`category`) |
|-------|------------------------|
| `ai.toml` | AI ve Kodlama |
| `git.toml` | Git ve Geliştirici Araçları |
| `files.toml` | Dosya Yönetimi ve Arama |
| `system.toml` | Sistem ve Kaynak Kullanımı |
| `network.toml` | Ağ ve Bağlantı |
| `docker.toml` | Docker ve Sunucu |
| `helpers.toml` | Günlük Terminal Yardımcıları |
| `custom.toml` | Özel Araçlar (gömülü boş; kullanıcı `config.toml`) |

Kullanıcı özelleştirmesi: `~/.config/nova/config.toml` (`id` ile birleştirilir).
