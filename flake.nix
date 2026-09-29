{
  description = "Hockey Stats: InStat PDF analyzer (build tools, incl. macOS cross-compilation)";

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
        targets = [ "aarch64-apple-darwin" "x86_64-apple-darwin" ];
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
        ];
      };
    };
}
