# Windows packaging

CI on `windows-latest` builds `x86_64-pc-windows-msvc` and ships:

- `nova-vX.Y.Z-x86_64-pc-windows-msvc.zip` containing `nova.exe`

Local pack (after `cargo build --release`):

```powershell
.\packaging\windows\pack.ps1 -Tag v0.2.0 -Binary target\release\nova.exe -OutDir dist
```

Nova remains primarily a Linux TUI. On Windows, terminal backends and some
store package-manager paths (pacman/dnf/apt) are unavailable; brew/cargo/npm
and archive installs still work where tools exist.
