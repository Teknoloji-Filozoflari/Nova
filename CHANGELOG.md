# Changelog

All notable changes to Nova are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [0.2.0] — 2026-11-22

### Added

- Multi-platform GitHub Release packages:
  - Linux `x86_64` + `aarch64` `.tar.gz`
  - Debian/Ubuntu `.deb` (amd64 + arm64)
  - Fedora/RHEL-like `.rpm` (x86_64 + aarch64)
  - Nix expression (`packaging/nix`, flake + classic)
  - Windows `x86_64` `.zip` (`nova.exe`)
  - macOS `x86_64` + `aarch64` `.tar.gz`
- `SHA256SUMS.txt` for every release asset; optional minisign when
  `MINISIGN_PRIVATE_KEY` is configured (unsigned packages still publish).
- Self-update asset picker selects host OS/arch (Linux aarch64, Windows,
  macOS) while keeping the Linux x86_64 path unchanged.
- Zip archive extraction for Windows self-update installs.

### Changed

- Release workflow builds a matrix of native/cross targets instead of a
  single Linux x86_64 tarball.
- Versioning docs and README install section cover Debian, Fedora, NixOS,
  Windows, and macOS download/install commands.

### Notes

- Nova remains a **Linux-first** TUI. Windows/macOS builds ship for
  convenience; Linux package-manager store paths (pacman/dnf/apt) are not
  available there — brew/cargo/npm/archive flows still apply when tools exist.
- Arch/pacman stays the preferred **runtime** package manager on Arch hosts;
  deb/rpm/nix exist for distribution reach.

## [0.1.0] — 2026-11-01

### Added

- Initial GitHub Release: `nova-v0.1.0-x86_64-unknown-linux-gnu.tar.gz` +
  `SHA256SUMS.txt`.
- TUI launcher, store, commands guide, settings, and self-update scaffolding.

[0.2.0]: https://github.com/Teknoloji-Filozoflari/Nova/releases/tag/v0.2.0
[0.1.0]: https://github.com/Teknoloji-Filozoflari/Nova/releases/tag/v0.1.0
