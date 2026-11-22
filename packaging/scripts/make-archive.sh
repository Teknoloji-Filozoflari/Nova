#!/usr/bin/env bash
# Stage nova binary + licenses into a release archive.
# Usage: make-archive.sh <tag> <target> <binary-path> <out-dir> [format]
# format: tar.gz (default) | zip
set -euo pipefail

TAG="${1:?tag}"
TARGET="${2:?target}"
BINARY="${3:?binary}"
OUT_DIR="${4:?out-dir}"
FORMAT="${5:-tar.gz}"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

STAGE_NAME="nova-${TAG}-${TARGET}"
STAGE="${OUT_DIR}/${STAGE_NAME}"
rm -rf "$STAGE"
mkdir -p "$STAGE"

BIN_BASENAME="$(basename "$BINARY")"
cp "$BINARY" "$STAGE/${BIN_BASENAME}"
chmod 755 "$STAGE/${BIN_BASENAME}" 2>/dev/null || true

cp "$ROOT/LICENSE.md" "$ROOT/LICENSE-MIT" "$ROOT/LICENSE-APACHE" "$ROOT/README.md" "$STAGE/"

mkdir -p "$OUT_DIR"
case "$FORMAT" in
  tar.gz|tgz)
    ASSET="${STAGE_NAME}.tar.gz"
    tar -C "$OUT_DIR" -czf "${OUT_DIR}/${ASSET}" "$STAGE_NAME"
    ;;
  zip)
    ASSET="${STAGE_NAME}.zip"
    if command -v zip >/dev/null 2>&1; then
      (cd "$OUT_DIR" && zip -r "$ASSET" "$STAGE_NAME")
    else
      python3 -c "
import pathlib, zipfile, sys
root = pathlib.Path(sys.argv[1])
out = pathlib.Path(sys.argv[2])
with zipfile.ZipFile(out, 'w', zipfile.ZIP_DEFLATED) as zf:
    for path in root.rglob('*'):
        if path.is_file():
            zf.write(path, path.relative_to(root.parent).as_posix())
" "$STAGE" "${OUT_DIR}/${ASSET}"
    fi
    ;;
  *)
    echo "unknown format: $FORMAT" >&2
    exit 1
    ;;
esac

# Artifact upload should only see the archive, not the stage tree.
rm -rf "$STAGE"

echo "$ASSET"
