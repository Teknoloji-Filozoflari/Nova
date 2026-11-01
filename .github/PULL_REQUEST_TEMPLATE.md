## Değişiklik


## Gerekçe


## Doğrulama

- [ ] `cargo fmt --check`
- [ ] `cargo clippy --locked --all-targets --all-features -- -D warnings`
- [ ] `cargo test --locked`
- [ ] Bağımlılık/`deny.toml` değiştiyse: `cargo audit` ve `cargo deny check`
- [ ] README veya mimari belgeleri gerektiğinde güncellendi

## İnceleme notları

- [ ] Kullanıcı girdisi shell komutuna birleştirilmiyor
- [ ] Yeni bağımlılık varsa gerekçesi ve MSRV etkisi açıklandı
- [ ] Örneklerden anahtar, kişisel yol ve makineye özel bilgiler çıkarıldı
- [ ] Gerçek ortamda denenmeyen davranışlar doğrulanmış gibi yazılmadı
