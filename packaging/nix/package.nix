{
  lib,
  rustPlatform,
  src,
}:

rustPlatform.buildRustPackage {
  pname = "nova";
  version = "0.2.0";

  inherit src;

  cargoLock.lockFile = src + "/Cargo.lock";

  # Tooling binary is not part of the Nix package.
  cargoBuildFlags = [
    "--bin"
    "nova"
  ];
  cargoTestFlags = [
    "--lib"
  ];

  meta = with lib; {
    description = "TUI launcher for terminal applications";
    homepage = "https://github.com/Teknoloji-Filozoflari/Nova";
    license = with licenses; [
      mit
      asl20
    ];
    mainProgram = "nova";
    # Primarily a Linux TUI; Darwin builds are best-effort.
    platforms = platforms.unix;
  };
}
