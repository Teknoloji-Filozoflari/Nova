# Nova ekran görüntüleri

Bu klasördeki PNG’ler README ürün sayfasında kullanılır. Hepsi **gerçek
`nova::ui::draw` çıktısıdır** — elle çizilmiş mock değil.

## Nasıl üretildi

Headless ortamda gerçek bir PTY / Konsole yok; bunun yerine uygulamadaki
`TestBackend` yolunu (entegrasyon testleriyle aynı) kullanıyoruz:

1. `tools/capture_screens.rs` — `App` durumunu ana liste / Mağaza / Ayarlar /
   AI aksiyon ekranlarına getirir, `ui::draw` ile çizer, hücreleri JSON’a yazar.
2. `docs/screenshots/render_frames.py` — JSON hücrelerini monospace PNG’ye
   rasterize eder (DejaVu Sans Mono).

```bash
cargo run --bin capture-screens -- docs/screenshots/frames
python3 docs/screenshots/render_frames.py docs/screenshots/frames docs/screenshots
```

Kurulu görünüm için yakalayıcı, katalogda seçili araçları
`InstalledStatus::Installed` işaretler (PATH taraması yapmaz). Mağaza ve
Ayarlar metinleri gömülü katalog / i18n tablolarından gelir.

`frames/*.json` ara çıktıdır; isteğe bağlı olarak commit edilmez (yeniden
üretilebilir). PNG’ler README için kalıcıdır.
