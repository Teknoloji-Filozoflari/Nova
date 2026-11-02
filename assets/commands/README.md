# Nova-Commands bank (curated)

Gömülü artifact'lar [Nova-Commands](https://github.com/Teknoloji-Filozoflari/Nova-Commands)
`metadata/` çıktısından kopyalanır. Sözleşme:
[`docs/NOVA_INTEGRATION.md`](https://github.com/Teknoloji-Filozoflari/Nova-Commands/blob/main/docs/NOVA_INTEGRATION.md).

| Dosya | `kind` | Rol |
|-------|--------|-----|
| `catalog.json` | `nova-commands-bank` | Kürate komut + konsept bankası |
| `manifest.json` | `nova-commands-manifest` | `content_sha256` + sayımlar |
| `search-index.json` | `nova-commands-search-index` | Arama alanları |

Bu dizin **Mağaza** (`assets/store/`) ve **kurulu araç kataloğu**
(`assets/catalog/`) ile karıştırılmaz.

**Sistemimde Bulunan Komutlar** (PATH keşfi) buraya yazılmaz; istemci
tarafında `kind: nova-local-commands` stub'ı üretir
([local-discovery-contract](https://github.com/Teknoloji-Filozoflari/Nova-Commands/blob/main/docs/local-discovery-contract.md)).
