# Katkı

Katkılar memnuniyetle karşılanır. Nova `MIT OR Apache-2.0` (SPDX) çift
lisanslıdır; kullanıcı iki lisanstan birini seçer (bkz. `LICENSE.md`,
`LICENSE-MIT`, `LICENSE-APACHE`). Katkı göndererek, katkınızın her iki lisans
koşulu altında projeye dahil edilip dağıtılmasına izin vermiş olursunuz. Ayrı
bir yazılı lisans anlaşması varsa o anlaşma geçerlidir.

## Kurulum

```bash
git clone <depo-adresi>
cd nova
cargo build
cargo test
```

## Göndermeden önce

CI'daki `check` işi şu komutları çalıştırır:

```bash
cargo fmt --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked
```

Bağımlılık veya `deny.toml` değiştiyse `security` işi de geçer olmalı:

```bash
cargo audit
cargo deny check
```

## Kurallar

- Katalog (`assets/catalog/*.toml`) ve araç modelleri UI'dan bağımsız kalır;
  `catalog` modülü `ratatui`/`crossterm` tanımaz.
- Program ve argümanlar `std::process::Command` ile **ayrı ayrı** geçirilir;
  kullanıcı girdisinden kabuk komutu birleştirilmez, `sudo` kullanılmaz.
- Testler gerçek terminal sekmesi açmaz, gerçek Konsole başlatmaz, kullanıcının
  gerçek yapılandırma ve durum dosyalarına yazmaz. Davranış testleri `handle_key`
  ve enjekte edilen adapterler üzerinden yapılır.
- Yeni bağımlılık eklerken gerekçeyi, lisans etkisini ve MSRV etkisini yazın.
- Davranış değişikliği varsa `README.md`; tasarım kararı varsa
  `ARCHITECTURE.md` aynı değişiklikte güncellenir.
- Bir şeyi "doğrulandı" diye yazmadan önce gerçekten çalıştırın; doğrulanmamış
  aşamaları doğrulanmamış olarak işaretleyin.

## Commit mesajı

`<kapsam>: <kısa emir kipi açıklama>` biçimi önerilir, ör. `launcher: proje
dizini geçersiz kılmayı düzelt`.
