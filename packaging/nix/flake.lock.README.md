# Nix flake lock

`flake.lock` is produced by `nix flake lock` / `nix build` when a Nix toolchain
is available. CI uploads this directory as
`nova-vX.Y.Z-nix-expr.tar.gz` so Releases always include the expression even
when lockfiles are regenerated on the consumer side.

```bash
nix flake lock
nix build
nix run
```
