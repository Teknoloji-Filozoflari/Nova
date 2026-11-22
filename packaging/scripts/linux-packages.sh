#!/usr/bin/env bash
# Build .deb and .rpm from a staged nova binary using nfpm.
# Usage: linux-packages.sh <version> <target-triple> <binary-path> <out-dir>
set -euo pipefail

VERSION="${1:?version}"
TARGET="${2:?target}"
BINARY="${3:?binary}"
OUT_DIR="${4:?out-dir}"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

case "$TARGET" in
  x86_64-*) DEB_ARCH=amd64; RPM_ARCH=x86_64 ;;
  aarch64-*|arm64-*) DEB_ARCH=arm64; RPM_ARCH=aarch64 ;;
  *)
    echo "unsupported target for deb/rpm: $TARGET" >&2
    exit 1
    ;;
esac

mkdir -p "$OUT_DIR"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

render() {
  local arch="$1"
  local out="$2"
  sed \
    -e "s|\${ARCH}|${arch}|g" \
    -e "s|\${VERSION}|${VERSION}|g" \
    -e "s|\${BINARY}|${BINARY}|g" \
    -e "s|\${LICENSE_MIT}|${ROOT}/LICENSE-MIT|g" \
    -e "s|\${LICENSE_APACHE}|${ROOT}/LICENSE-APACHE|g" \
    -e "s|\${README}|${ROOT}/README.md|g" \
    "${ROOT}/packaging/nfpm.yaml.tmpl" >"$out"
}

DEB_CFG="$TMP/nfpm-deb.yaml"
RPM_CFG="$TMP/nfpm-rpm.yaml"
render "$DEB_ARCH" "$DEB_CFG"
render "$RPM_ARCH" "$RPM_CFG"

nfpm package --config "$DEB_CFG" --packager deb --target "$OUT_DIR"
nfpm package --config "$RPM_CFG" --packager rpm --target "$OUT_DIR"

# Normalize asset names for GitHub Releases / self-update pickers.
shopt -s nullglob
for f in "$OUT_DIR"/nova_"${VERSION}"_*.deb "$OUT_DIR"/nova-"${VERSION}"-*.deb; do
  [ -f "$f" ] || continue
  mv -f "$f" "$OUT_DIR/nova-v${VERSION}-${TARGET}.deb"
done
for f in "$OUT_DIR"/nova-"${VERSION}"-*.rpm "$OUT_DIR"/nova-${VERSION}-1.*.rpm; do
  [ -f "$f" ] || continue
  case "$f" in
    *.deb) continue ;;
  esac
  mv -f "$f" "$OUT_DIR/nova-v${VERSION}-${TARGET}.rpm"
done

ls -la "$OUT_DIR"/nova-v"${VERSION}"-"${TARGET}".{deb,rpm}
