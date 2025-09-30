{
  description = "Chatmail TURN server";
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    naersk.url = "github:nix-community/naersk";
    nix-filter.url = "github:numtide/nix-filter";
    flake-utils.url = "github:numtide/flake-utils";
    fenix.url = "github:nix-community/fenix";
  };
  outputs = {
    self,
    nixpkgs,
    flake-utils,
    fenix,
    naersk,
    nix-filter,
  }:
    flake-utils.lib.eachDefaultSystem (
      system: let
        pkgs = nixpkgs.legacyPackages.${system};
        manifest = (pkgs.lib.importTOML ./Cargo.toml).package;
        fenixPkgs = fenix.packages.${system};
        rustSrc = nix-filter.lib {
          root = ./.;
          include = [
            ./Cargo.toml
            ./Cargo.lock
            ./src
          ];
        };
        arch2target = {
          "x86_64-linux" = "x86_64-unknown-linux-musl";
          "aarch64-linux" = "aarch64-unknown-linux-musl";
        };

        mkCrossRustPackage = arch: let
          target = arch2target."${arch}";
          pkgsCross = import nixpkgs {
            system = system;
            crossSystem.config = "${target}";
          };
        in let
          toolchain = fenixPkgs.combine [
            fenixPkgs.stable.rustc
            fenixPkgs.stable.cargo
            fenixPkgs.targets.${target}.stable.rust-std
          ];
          naersk-lib = pkgs.callPackage naersk {
            cargo = toolchain;
            rustc = toolchain;
          };
        in
          naersk-lib.buildPackage rec {
            pname = "chatmail-turn";
            version = manifest.version;
            strictDeps = true;
            src = rustSrc;
            doCheck = false;

            CARGO_BUILD_TARGET = target;
            TARGET_CC = "${pkgsCross.stdenv.cc}/bin/${pkgsCross.stdenv.cc.targetPrefix}cc";
            CARGO_BUILD_RUSTFLAGS = [
              "-C"
              "linker=${TARGET_CC}"
            ];

            CC = "${pkgsCross.stdenv.cc}/bin/${pkgsCross.stdenv.cc.targetPrefix}cc";
            LD = "${pkgsCross.stdenv.cc}/bin/${pkgsCross.stdenv.cc.targetPrefix}cc";
          };
      in {
        formatter = pkgs.alejandra;
        packages = {
          "chatmail-turn-aarch64-linux" = mkCrossRustPackage "aarch64-linux";
          "chatmail-turn-x86_64-linux" = mkCrossRustPackage "x86_64-linux";
        };
      }
    );
}
