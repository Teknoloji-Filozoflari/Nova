# macOS packaging

CI builds on `macos-latest` (Apple Silicon):

- `nova-vX.Y.Z-aarch64-apple-darwin.tar.gz` (native)
- `nova-vX.Y.Z-x86_64-apple-darwin.tar.gz` (cross target on the same runner)

`macos-13` / `macos-14` labels are avoided: this account reported
`macos-14-arm64-us-default is not supported`, and `macos-13` stayed queued.

```bash
./packaging/scripts/make-archive.sh v0.2.0 aarch64-apple-darwin \
  target/release/nova dist tar.gz
```

Nova is primarily a Linux TUI. On macOS, Linux package managers are gated;
Homebrew (`brew`) and cargo/npm paths remain available when present.
