# Nova-Store katalog genel anahtarı

`nova-store-catalog.pub` — minisign public key. Nova, uzak
`store-catalog.toml` yayınını yalnızca bu anahtarla doğrular
(`src/store/remote.rs`).

İmza dosyası: `store-catalog.toml.minisig` (GitHub Releases asset).
Gizli anahtar Nova deposunda **tutulmaz**; Nova-Store yayın CI’sinde saklanır.
