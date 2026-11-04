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

### 2) Çalışma zamanı (isteğe bağlı — Ayarlar)

Ayarlar → **Komut bankası**: Nova-Commands `main` ham HTTPS URL’lerinden
manifest (+ gerekirse catalog/search-index) çeker.

- İmzasızdır; güven = sabit GitHub raw URL + HTTPS + `kind` / `content_sha256`.
- Aynı SHA ise catalog indirmez (“güncel”).
- Başarıda `~/.local/share/nova/commands/` (veya `NOVA_COMMANDS_DIR`) önbelleğine yazar.
- Açılışta geçerli önbellek gömülünün önüne geçer; ağ çağrısı yok.
- Mağaza (`assets/store`, Releases imzası) bu yoldan **etkilenmez**.

Ortam değişkenleri: `NOVA_COMMANDS_MANIFEST_URL`, `NOVA_COMMANDS_CATALOG_URL`,
`NOVA_COMMANDS_SEARCH_INDEX_URL`, `NOVA_COMMANDS_DISABLE_REMOTE=1`,
`NOVA_COMMANDS_DIR`.
