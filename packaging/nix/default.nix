# Classic Nix expression (non-flake). Prefer packaging/nix/flake.nix.
{ pkgs ? import <nixpkgs> { } }:

pkgs.callPackage ./package.nix {
  src = ../..;
}
