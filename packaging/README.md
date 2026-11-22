# Nova packaging

Deterministic helpers used by `.github/workflows/release.yml`.

| Path | Purpose |
|------|---------|
| `scripts/make-archive.sh` | tar.gz / zip stage for GitHub Releases |
| `scripts/linux-packages.sh` | `.deb` + `.rpm` via nfpm |
| `nfpm.yaml.tmpl` | Shared nfpm metadata |
| `nix/` | Flake + classic `default.nix` |
| `windows/pack.ps1` | Windows zip helper |
| `macos/` | macOS archive notes |
| `deb/`, `rpm/` | Distro install notes |

## Release assets (v0.2.0+)

- `nova-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz`
- `nova-vX.Y.Z-aarch64-unknown-linux-gnu.tar.gz`
- `nova-vX.Y.Z-x86_64-unknown-linux-gnu.deb` / `.rpm`
- `nova-vX.Y.Z-aarch64-unknown-linux-gnu.deb` / `.rpm`
- `nova-vX.Y.Z-x86_64-pc-windows-msvc.zip`
- `nova-vX.Y.Z-x86_64-apple-darwin.tar.gz`
- `nova-vX.Y.Z-aarch64-apple-darwin.tar.gz`
- `nova-vX.Y.Z-nix-expr.tar.gz`
- `SHA256SUMS.txt`

## Optional minisign

Unsigned packages are published whenever a tag build succeeds. To also attach
`.minisig` files, set repository secrets:

| Secret | Meaning |
|--------|---------|
| `MINISIGN_PRIVATE_KEY` | minisign secret key (file contents) |
| `MINISIGN_PASSWORD` | key password (empty string if unencrypted) |

Public key should be documented in release notes / README when signing is enabled.
Without these secrets the release job skips signing and still uploads checksums.

## Runner notes

- Linux + Windows use `ubuntu-latest` / `windows-latest`.
- macOS uses `macos-latest` only (native aarch64 + cross `x86_64-apple-darwin`).
  macOS matrix cells are `continue-on-error` so a runner outage does not block
  publishing Linux/Windows/Nix assets.
