#!/usr/bin/env bash
# Vendor Nova-Store catalog into assets/store/*.toml (embedded Magaza fallback).
#
# Usage (from Nova repo root):
#   ./tools/sync_store_catalog.sh
#   NOVA_STORE_REF=main ./tools/sync_store_catalog.sh
#   NOVA_STORE_SRC=/path/to/Nova-Store ./tools/sync_store_catalog.sh
#
# Converts Nova-Store `apps/<slug>/*.toml` (+ optional repository_url) into
# Nova's category files (`[[apps]]` + Turkish category labels). Does not touch
# assets/commands or assets/catalog. Runtime still prefers signed Releases;
# this only refreshes the offline/embedded baseline.
#
# Prefer running after a Nova-Store expansion PR merges so the embedded
# snapshot matches the signed publish channel.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="${ROOT}/assets/store"
REF="${NOVA_STORE_REF:-main}"
REPO_URL="${NOVA_STORE_REPO_URL:-https://github.com/Teknoloji-Filozoflari/Nova-Store}"

mkdir -p "${DEST}"

if [[ -n "${NOVA_STORE_SRC:-}" ]]; then
  SRC="${NOVA_STORE_SRC%/}"
  if [[ ! -d "${SRC}/apps" ]]; then
    echo "error: ${SRC}/apps yok" >&2
    exit 1
  fi
  echo "Kaynak: yerel ${SRC}"
else
  TMP="$(mktemp -d)"
  trap 'rm -rf "${TMP}"' EXIT
  ARCHIVE_URL="${REPO_URL}/archive/refs/heads/${REF}.tar.gz"
  echo "Kaynak: ${ARCHIVE_URL}"
  curl -fsSL "${ARCHIVE_URL}" | tar -xz -C "${TMP}"
  SRC="$(find "${TMP}" -maxdepth 1 -type d -name 'Nova-Store-*' | head -n1)"
  if [[ -z "${SRC}" || ! -d "${SRC}/apps" ]]; then
    echo "error: Nova-Store arşivinden apps/ çıkarılamadı" >&2
    exit 1
  fi
fi

python3 - <<'PY' "${SRC}" "${DEST}"
import sys
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib  # type: ignore

src = Path(sys.argv[1])
dest = Path(sys.argv[2])

# Nova-Store slug → (assets/store file stem, Turkish STORE_CATEGORIES label)
CAT = {
    "ai": ("ai", "AI"),
    "git": ("git", "Git"),
    "files": ("files", "Dosya ve Arama"),
    "development": ("dev", "Geliştirme"),
    "editors": ("editors", "Editörler"),
    "system": ("system", "Sistem"),
    "panels": ("panels", "Paneller"),
    "networking": ("network", "Ağ"),
    "containers": ("container", "Konteyner"),
    "database": ("database", "Veritabanı"),
    "security": ("security", "Güvenlik"),
    "media": ("media", "Medya"),
    "messaging": ("messaging", "Mesajlaşma"),
    "productivity": ("productivity", "Verimlilik"),
    "games": ("games", "Oyunlar"),
    "screensavers": ("screensavers", "Ekran Koruyucular"),
    "web": ("web", "Web"),
    "daily": ("daily", "Günlük"),
}

KEEP = {
    "id",
    "name",
    "description",
    "source",
    "license",
    "executable",
    "install_method",
    "supported_platforms",
    "checksum",
    "checksum_type",
    "version",
    "download_url",
    "install_command",
    "install_notes",
    "repository_url",
    "artifacts",
    "release",
    "signature",
}

# Drop Nova-Store-only fields Nova derives or ignores at embed time.
DROP = {"privilege", "trust_class", "risk_note", "category"}


def toml_str(s: str) -> str:
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n") + '"'


def emit_value(v, indent=0):
    sp = " " * indent
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, (int, float)):
        return str(v)
    if isinstance(v, str):
        if "\n" in v:
            # multi-line basic string
            return '"""\n' + v.replace('"""', '\\"""') + '\n"""'
        return toml_str(v)
    if isinstance(v, list):
        if not v:
            return "[]"
        if all(isinstance(x, str) for x in v):
            inner = ", ".join(toml_str(x) for x in v)
            return f"[{inner}]"
        # complex arrays handled as tables by caller
        raise TypeError("nested list")
    raise TypeError(type(v))


def emit_table(prefix: str, data: dict) -> list[str]:
    lines = [f"[{prefix}]"]
    for k, v in data.items():
        if isinstance(v, dict):
            continue
        if isinstance(v, list) and v and isinstance(v[0], dict):
            continue
        lines.append(f"{k} = {emit_value(v)}")
    return lines


def emit_app(app: dict, nova_label: str) -> str:
    lines = ["[[apps]]"]
    order = [
        "id",
        "name",
        "category",
        "description",
        "source",
        "license",
        "executable",
        "install_method",
        "supported_platforms",
        "version",
        "download_url",
        "checksum",
        "checksum_type",
        "install_command",
        "install_notes",
        "repository_url",
    ]
    app = dict(app)
    app["category"] = nova_label
    for key in order:
        if key not in app or app[key] is None:
            continue
        val = app[key]
        if key == "install_notes" and isinstance(val, str) and "\n" in val:
            lines.append(f"{key} = {emit_value(val)}")
        else:
            lines.append(f"{key} = {emit_value(val)}")
    for art in app.get("artifacts") or []:
        lines.append("")
        lines.append("[[apps.artifacts]]")
        for k, v in art.items():
            if v is None:
                continue
            lines.append(f"{k} = {emit_value(v)}")
    if app.get("release"):
        lines.append("")
        lines.append("[apps.release]")
        for k, v in app["release"].items():
            if v is None:
                continue
            lines.append(f"{k} = {emit_value(v)}")
    if app.get("signature"):
        lines.append("")
        lines.append("[apps.signature]")
        for k, v in app["signature"].items():
            if v is None:
                continue
            lines.append(f"{k} = {emit_value(v)}")
    return "\n".join(lines) + "\n"


by_file: dict[str, list[str]] = {stem: [] for stem, _ in CAT.values()}
counts = {stem: 0 for stem, _ in CAT.values()}
total = 0
skipped = 0

for path in sorted((src / "apps").rglob("*.toml")):
    raw = path.read_bytes()
    data = tomllib.loads(raw.decode("utf-8"))
    slug = data.get("category") or path.parent.name
    if slug not in CAT:
        print(f"skip unknown category {slug}: {path}", file=sys.stderr)
        skipped += 1
        continue
    stem, label = CAT[slug]
    cleaned = {k: v for k, v in data.items() if k in KEEP and k not in DROP}
    # Normalize repository_url if present but empty
    if cleaned.get("repository_url") == "":
        cleaned.pop("repository_url", None)
    by_file[stem].append(emit_app(cleaned, label))
    counts[stem] += 1
    total += 1

HEADER = """# Nova mağaza kataloğu — {label}
#
# Kategori: "{label}"
# `assets/store/` altında; loader tüm kategori dosyalarını birleştirir.
# Kullanıcı düzenlemez. Kaynak: Nova-Store sync (`tools/sync_store_catalog.sh`).
# download_archive için https + SHA-256 zorunlu.
# İsteğe bağlı: artifacts, release, repository_url, signature.

"""

for slug, (stem, label) in CAT.items():
    out = dest / f"{stem}.toml"
    body = HEADER.format(label=label) + "\n".join(by_file[stem])
    out.write_text(body, encoding="utf-8")
    print(f"wrote {out.relative_to(dest.parent.parent)} ({counts[stem]} apps)")

print(f"OK: {total} apps, skipped={skipped}")
if total < 1:
    raise SystemExit("no apps written")
PY

echo "Yazıldı: ${DEST}/*.toml"
echo "Sözleşme: https://github.com/Teknoloji-Filozoflari/Nova-Store/blob/main/docs/nova-consumption.md"
echo "Sonraki: cargo +stable test --locked --lib store::"
