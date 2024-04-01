{
  description = ''
    Simple request-response-notify bidirectional binary protocol implementation
    based on async
  '';
  inputs = {
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs = {
      nixpkgs.follows = "nixpkgs";
      flake-utils.follows = "flake-utils";
    };
    # TODO(Shvedov) Required unstable channel fot rust >= 1.70
    nixpkgs.url = github:NixOS/nixpkgs/nixos-23.11;
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        buildInputs = [ ];
        nativeBuildInputs = [ ];
        rust = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
      in
      rec {
        packages = rec {
          parity = pkgs.rustPlatform.buildRustPackage rec {
            pname = "parity";
            version = "0.1.0";
            inherit nativeBuildInputs buildInputs;
            src = with builtins; path {
              filter = (path: type:
                let
                  bn = baseNameOf path;
                in
                bn != "flake.nix" && bn != "flake.lock"
              );
              path = self;
            };
            cargoLock.lockFile = "${self}/Cargo.lock";
          };
          default = parity;
        };
        devShells.default = with pkgs; mkShellNoCC {
          buildInputs = [
            rust
            #rust-analyzer-unwrapped
          ] ++ buildInputs ++ nativeBuildInputs;
          RUST_SRC_PATH = "${rust}/lib/rustlib/src/rust/library";
        };
        formatter = pkgs.alejandra;
      }
    );
}
