# macOS packaging

CI builds:

- `nova-vX.Y.Z-x86_64-apple-darwin.tar.gz` (`macos-13`)
- `nova-vX.Y.Z-aarch64-apple-darwin.tar.gz` (`macos-14`)

```bash
./packaging/scripts/make-archive.sh v0.2.0 aarch64-apple-darwin \
  target/release/nova dist tar.gz
```

Nova is primarily a Linux TUI. On macOS, Linux package managers are gated;
Homebrew (`brew`) and cargo/npm paths remain available when present.
