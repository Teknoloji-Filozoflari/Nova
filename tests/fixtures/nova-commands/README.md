# Nova-Commands signed-bank fixtures

Minimal `catalog.json` / `manifest.json` / `search-index.json` plus detached
minisign signatures for **unit tests** (`src/commands/remote.rs`).

| File | Role |
|------|------|
| `*.json` | Schema v2 bank triplet (`kind` contract) |
| `*.minisig` | Detached signatures |
| `minisign.pub` / `minisign.pub.b64` | **Test-only** public key |

These signatures are **not** produced with the production Nova-Commands key
(`assets/keys/nova-commands-catalog.pub`). Production verify uses the pinned
pubkey; fixture crypto tests pass the test key explicitly.
