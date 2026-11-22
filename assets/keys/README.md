# Katalog genel anahtarları (minisign)

| Dosya | Rol |
|-------|-----|
| `nova-store-catalog.pub` | Nova-Store imzalı `store-catalog.toml` (`src/store/remote.rs`) |
| `nova-commands-catalog.pub` | Nova-Commands imzalı banka (`catalog.json` / `manifest.json` / `search-index.json`; `src/commands/remote.rs`) |

Gizli anahtarlar Nova deposunda **tutulmaz**. Store anahtarı Nova-Store CI’de;
Commands anahtarı Nova-Commands `MINISIGN_SECRET_KEY` Actions secret’ında
(kaynak: [Nova-Commands `keys/minisign.pub`](https://github.com/Teknoloji-Filozoflari/Nova-Commands/blob/main/keys/minisign.pub)).

Anahtar döndürme: SSOT depodaki pubkey + Nova pin’i birlikte güncellenir.
