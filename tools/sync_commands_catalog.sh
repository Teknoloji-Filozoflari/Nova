#!/usr/bin/env bash
# Vendor Nova-Commands main metadata into assets/commands/.
#
# Usage (from Nova repo root):
#   ./tools/sync_commands_catalog.sh
#   NOVA_COMMANDS_REF=main ./tools/sync_commands_catalog.sh
#   NOVA_COMMANDS_SRC=/path/to/Nova-Commands ./tools/sync_commands_catalog.sh
#
# Does not touch Mağaza (assets/store) or curated tool catalog (assets/catalog).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="${ROOT}/assets/commands"
REF="${NOVA_COMMANDS_REF:-main}"
REPO_URL="${NOVA_COMMANDS_REPO_URL:-https://github.com/Teknoloji-Filozoflari/Nova-Commands}"

mkdir -p "${DEST}"

if [[ -n "${NOVA_COMMANDS_SRC:-}" ]]; then
  SRC="${NOVA_COMMANDS_SRC%/}"
  if [[ ! -f "${SRC}/metadata/catalog.json" ]]; then
    echo "error: ${SRC}/metadata/catalog.json yok" >&2
    exit 1
  fi
  echo "Kaynak: yerel ${SRC}"
  cp -f "${SRC}/metadata/catalog.json" "${DEST}/catalog.json"
  cp -f "${SRC}/metadata/manifest.json" "${DEST}/manifest.json"
  cp -f "${SRC}/metadata/search-index.json" "${DEST}/search-index.json"
else
  TMP="$(mktemp -d)"
  trap 'rm -rf "${TMP}"' EXIT
  ARCHIVE_URL="${REPO_URL}/archive/refs/heads/${REF}.tar.gz"
  echo "Kaynak: ${ARCHIVE_URL}"
  curl -fsSL "${ARCHIVE_URL}" | tar -xz -C "${TMP}"
  SRC="$(find "${TMP}" -maxdepth 1 -type d -name 'Nova-Commands-*' | head -n1)"
  cp -f "${SRC}/metadata/catalog.json" "${DEST}/catalog.json"
  cp -f "${SRC}/metadata/manifest.json" "${DEST}/manifest.json"
  cp -f "${SRC}/metadata/search-index.json" "${DEST}/search-index.json"
fi

python3 - <<'PY' "${DEST}"
import json, sys
dest = sys.argv[1]
man = json.load(open(f"{dest}/manifest.json"))
cat = json.load(open(f"{dest}/catalog.json"))
idx = json.load(open(f"{dest}/search-index.json"))
assert man["kind"] == "nova-commands-manifest", man["kind"]
assert cat["kind"] == "nova-commands-bank", cat["kind"]
assert idx["kind"] == "nova-commands-search-index", idx["kind"]
assert "store" not in cat["kind"]
for doc, name in ((man, "manifest"), (cat, "catalog"), (idx, "search-index")):
    sv = doc.get("schema_version")
    assert sv in (1, 2), f"{name} schema_version={sv} (expected 1 or 2)"
sha = man["content_sha256"]
assert cat.get("content_sha256") == sha, "catalog SHA ≠ manifest"
assert idx.get("content_sha256") in (None, sha), "search-index SHA ≠ manifest"
print(f"OK: schema={man.get('schema_version')} {man.get('command_count')} commands, {man.get('concept_count')} concepts, sha={sha[:12]}…")
PY

echo "Yazıldı: ${DEST}/{catalog,manifest,search-index}.json"
echo "Sözleşme: https://github.com/Teknoloji-Filozoflari/Nova-Commands/blob/main/docs/NOVA_INTEGRATION.md"
