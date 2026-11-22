# Nova-Commands bank (curated)

Gömülü artifact'lar [Nova-Commands](https://github.com/Teknoloji-Filozoflari/Nova-Commands)
`metadata/` çıktısından kopyalanır. Sözleşme:
[`docs/NOVA_INTEGRATION.md`](https://github.com/Teknoloji-Filozoflari/Nova-Commands/blob/main/docs/NOVA_INTEGRATION.md).

| Dosya | `kind` | Rol |
|-------|--------|-----|
| `catalog.json` | `nova-commands-bank` | Kürate komut + konsept bankası |
| `manifest.json` | `nova-commands-manifest` | `content_sha256` + sayımlar |
| `search-index.json` | `nova-commands-search-index` | Arama alanları |

Canlı banka **`schema_version: 2`** (Nova-Commands Faz 3). Nova `1` ve `2`
okur; kurulum ipuçlarında `package_refs` tercih edilir, legacy `packages.*`
geçiş için kalır. Dual-concept korunur: bu dizin ≠ PATH keşfi
(`nova-local-commands`).

Bu dizin **Mağaza** (`assets/store/`) ve **kurulu araç kataloğu**
(`assets/catalog/`) ile karıştırılmaz.

**Sistemimde Bulunan Komutlar** (PATH keşfi) buraya yazılmaz; istemci
tarafında `kind: nova-local-commands` stub'ı üretir
([local-discovery-contract](https://github.com/Teknoloji-Filozoflari/Nova-Commands/blob/main/docs/local-discovery-contract.md)).

## Yenileme yolları

### 1) Vendor kopyası (bu dizin — release / PR)

Nova kökünden:

```bash
./tools/sync_commands_catalog.sh
# veya yerel clone:
# NOVA_COMMANDS_SRC=/path/to/Nova-Commands ./tools/sync_commands_catalog.sh
```

`main` (veya `NOVA_COMMANDS_REF`) üzerindeki `metadata/{catalog,manifest,search-index}.json`
dosyalarını buraya kopyalar; `kind` + `content_sha256` tutarlılığını doğrular.
Gömülü kopya bir sonraki Nova sürümüne / PR’ye gider.

### 2) Çalışma zamanı — imzalı Releases (tercih edilen)

Ayarlar → **Komut bankası**: önce Nova-Commands GitHub Releases
(`catalog-v*` / `releases/latest`) üzerinden `catalog.json` +
`manifest.json` + `search-index.json` ve `.minisig` asset’lerini çeker.

- Minisign doğrulama: gömülü `assets/keys/nova-commands-catalog.pub`
  (`src/commands/remote.rs`).
- İmza başarısız / 404 → mevcut güvenilen veya gömülü banka **korunur**.
- Başarıda `~/.local/share/nova/commands/trusted/` altına yazar.
- Açılışta imzalı güvenilen kopya, imzasız önbelleğin ve gömülünün önündedir.

### 3) Çalışma zamanı — imzasız `main` (yedek)

İmzalı yol başarısızsa (veya ek olarak) Nova-Commands `main` ham HTTPS
URL’lerinden manifest (+ gerekirse catalog/search-index) çekilir.

- **İmzasızdır**; UI durum satırında “imzasız main” diye etiketlenir.
- Güven = sabit GitHub raw URL + HTTPS + `kind` / `content_sha256`.
- Aynı SHA ise catalog indirmez (“güncel”).
- Başarıda `~/.local/share/nova/commands/` önbelleğine yazar (`trusted/` üzerine yazmaz).
- Mağaza (`assets/store`, Releases imzası) bu yoldan **etkilenmez**.

Ortam değişkenleri: `NOVA_COMMANDS_RELEASE_*_URL` /
`NOVA_COMMANDS_RELEASES_BASE`, `NOVA_COMMANDS_MANIFEST_URL`,
`NOVA_COMMANDS_CATALOG_URL`, `NOVA_COMMANDS_SEARCH_INDEX_URL`,
`NOVA_COMMANDS_DISABLE_REMOTE=1`, `NOVA_COMMANDS_DIR`.
