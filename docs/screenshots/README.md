# Nova ekran görüntüleri

Bu klasördeki PNG’ler README ürün sayfasında kullanılır. Hepsi **gerçek
`nova::ui::draw` çıktısıdır** — elle çizilmiş mock değil. Dil **tr**, tema
**varsayilan** (README ile aynı).

## Nasıl üretildi

Headless ortamda gerçek bir PTY / Konsole yok; bunun yerine uygulamadaki
`TestBackend` yolunu (entegrasyon testleriyle aynı) kullanıyoruz:

1. `tools/capture_screens.rs` — `App` durumunu ana liste / Mağaza / Ayarlar /
   Kurulu AI aksiyon modalı / Komut Rehberi (Başlangıç + görev detayı)
   ekranlarına getirir, `ui::draw` ile çizer, hücreleri JSON’a yazar.
2. `docs/screenshots/render_frames.py` — JSON hücrelerini monospace PNG’ye
   rasterize eder (DejaVu Sans Mono).

```bash
cargo run --locked --bin capture-screens -- docs/screenshots/frames
python3 docs/screenshots/render_frames.py docs/screenshots/frames docs/screenshots
```

| PNG | Sahne |
| --- | --- |
| `main-list.png` | Kurulu ana liste (Sistem) |
| `store.png` | Mağaza · Dosya ve Arama |
| `settings.png` | Ayarlar (Nova güncelle / katalog / banka / tema / dil) |
| `ai-actions.png` | Kurulu AI · Enter aksiyon modalı |
| `commands-start.png` | Komut Rehberi · Başlangıç (Mod+Liste+Detay) |
| `commands-task-detail.png` | Komut Rehberi · görev drill + yan detay (Grup+Liste+Detay) |

Komut Rehberi kareleri **en fazla 3 içerik paneli** kuralına uyar (#62): eski
Mod+Grup+Liste+Detay (4 sütun) düzeni yakalanmaz. Commit sonrası breadcrumb
görev yolunu gösterir; komutlar otomatik çalıştırılmaz.

Kurulu görünüm için yakalayıcı, katalogda seçili araçları
`InstalledStatus::Installed` işaretler (PATH taraması yapmaz). Mağaza ve
Ayarlar metinleri gömülü katalog / i18n tablolarından gelir.

`frames/*.json` ara çıktıdır; isteğe bağlı olarak commit edilmez (yeniden
üretilebilir). PNG’ler README için kalıcıdır.
