{
  description = "Hockey Stats: InStat PDF analyzer (build tools, incl. macOS and Windows cross-compilation)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { nixpkgs, rust-overlay, ... }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs {
        inherit system;
        overlays = [ rust-overlay.overlays.default ];
      };
      rust = pkgs.rust-bin.stable.latest.default.override {
        extensions = [ "clippy" "rustfmt" ];
        targets = [ "aarch64-apple-darwin" "x86_64-apple-darwin" "x86_64-pc-windows-gnu" "x86_64-unknown-linux-musl" ];
      };
      # Converts an ISO image into a compressed macOS .dmg (the approach Bitcoin Core uses
      # to build Mac disk images off macOS).
      libdmg-hfsplus = pkgs.stdenv.mkDerivation {
        pname = "libdmg-hfsplus";
        version = "unstable-1cc791e";
        src = pkgs.fetchFromGitHub {
          owner = "fanquake";
          repo = "libdmg-hfsplus";
          rev = "1cc791e4173da9cb0b0cc16c5a1aaa25d5eb5efa";
          hash = "sha256-FdpuRq6vmvM10RMILDVRYsDcu64ItKvjdfB4CmuU2UQ=";
        };
        nativeBuildInputs = [ pkgs.cmake ];
        buildInputs = [ pkgs.zlib pkgs.bzip2 ];
        installPhase = ''
          install -Dm755 dmg/dmg $out/bin/dmg
        '';
      };
    in
    {
      devShells.${system}.default = pkgs.mkShell {
        packages = [
          rust
          pkgs.zig
          pkgs.cargo-zigbuild
          pkgs.rcodesign
          pkgs.zip
          pkgs.xorriso
          pkgs.minisign
          pkgs.gh
          pkgs.librsvg
          pkgs.python3
          libdmg-hfsplus
        ];
      };
    };
}
