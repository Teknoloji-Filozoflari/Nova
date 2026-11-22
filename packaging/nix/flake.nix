{
  description = "Nova — TUI launcher for terminal applications";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    # Full Nova source (non-flake). Bump `ref` when cutting a release tag.
    # Local override: nix build --override-input nova-src path:../..
    nova-src = {
      url = "github:Teknoloji-Filozoflari/Nova?ref=main";
      flake = false;
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      nova-src,
    }:
    flake-utils.lib.eachSystem
      [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ]
      (
        system:
        let
          pkgs = import nixpkgs { inherit system; };
          nova = pkgs.callPackage ./package.nix {
            src = nova-src;
          };
        in
        {
          packages.default = nova;
          packages.nova = nova;
          apps.default = {
            type = "app";
            program = "${nova}/bin/nova";
          };
          apps.nova = self.apps.${system}.default;
          checks.nova = nova;
        }
      );
}
