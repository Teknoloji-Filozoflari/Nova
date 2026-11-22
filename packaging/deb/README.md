# Debian / Ubuntu (`.deb`)

Produced in CI via `packaging/scripts/linux-packages.sh` + nfpm from the same
Linux release binary.

Assets:

- `nova-vX.Y.Z-x86_64-unknown-linux-gnu.deb` (amd64)
- `nova-vX.Y.Z-aarch64-unknown-linux-gnu.deb` (arm64)

```bash
sudo apt install ./nova-v0.2.0-x86_64-unknown-linux-gnu.deb
```
