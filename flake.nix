{
  description = "Swet102 — opinionated SW102 display firmware for one e-bike (PET)";

  # nix develop                        dev shell: Rust (+thumbv6m), arm-none-eabi-gcc, SDK, openocd
  # nix run .#emu                      desktop emulator
  # nix build .#firmware               build/swet102.hex (+ .elf, .map), dev build
  # nix flake check                    tests, lints, firmware build + size gates (what CI runs)

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; overlays = [ (import rust-overlay) ]; };
        lib = pkgs.lib;
        version = "0.0.1";

        toolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        rustPlatform = pkgs.makeRustPlatform { cargo = toolchain; rustc = toolchain; };

        nrfSdk = pkgs.fetchzip {
          url = "https://developer.nordicsemi.com/nRF5_SDK/nRF5_SDK_v12.x.x/nRF5_SDK_12.3.0_d7731ad.zip";
          hash = "sha256-ybi2l998Pg/MijrBbIZAkbApv6WSNd+GvN0qoLu0/Ko=";
        };

        # minifb loads X11 at runtime.
        x11Libs = with pkgs; [ libx11 libxcursor libxrandr libxi ];

        src = lib.fileset.toSource {
          root = ./.;
          fileset = lib.fileset.unions [
            ./Cargo.toml ./Cargo.lock ./rust-toolchain.toml ./crates
            ./platform ./prebuilt ./tools ./Makefile ./version.mk
            ./assets/fonts # the emulator embeds W95FA for its panel text
          ];
        };

        emu = rustPlatform.buildRustPackage {
          pname = "swet-emu";
          inherit version src;
          cargoLock.lockFile = ./Cargo.lock;
          cargoBuildFlags = [ "-p" "swet-emu" ];
          cargoTestFlags = [ "-p" "swet-heart" "-p" "swet-sim" ];
          nativeBuildInputs = [ pkgs.makeWrapper pkgs.pkg-config ];
          buildInputs = x11Libs;
          postInstall = ''
            wrapProgram $out/bin/swet-emu --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath x11Libs}
          '';
          meta.mainProgram = "swet-emu";
        };

        firmware = pkgs.stdenv.mkDerivation {
          pname = "swet102-firmware";
          inherit version src;
          cargoDeps = rustPlatform.importCargoLock { lockFile = ./Cargo.lock; };
          nativeBuildInputs = [ toolchain rustPlatform.cargoSetupHook pkgs.gcc-arm-embedded pkgs.gnumake ];
          makeFlags = [ "SDK_ROOT=${nrfSdk}" "VERSION_NUM=0" ];
          buildFlags = [ "all" "check" ];
          enableParallelBuilding = true;
          dontFixup = true;
          installPhase = ''
            install -Dm644 -t $out build/swet102.hex build/swet102.elf build/swet102.map
          '';
        };

        lint = pkgs.stdenv.mkDerivation {
          pname = "swet102-lint";
          inherit version src;
          cargoDeps = rustPlatform.importCargoLock { lockFile = ./Cargo.lock; };
          nativeBuildInputs = [ toolchain rustPlatform.cargoSetupHook pkgs.pkg-config ];
          buildInputs = x11Libs;
          buildPhase = ''
            cargo fmt --all --check
            cargo clippy --offline --workspace --exclude swet-fw --all-targets -- -D warnings
            cargo clippy --offline -p swet-fw --target thumbv6m-none-eabi -- -D warnings
          '';
          installPhase = "touch $out";
        };
      in {
        packages = { inherit emu firmware; default = emu; };
        apps.emu = flake-utils.lib.mkApp { drv = emu; };
        checks = { inherit emu firmware lint; };

        devShells.default = pkgs.mkShell {
          packages = [
            toolchain pkgs.gcc-arm-embedded pkgs.gnumake pkgs.srecord pkgs.openocd
            pkgs.pkg-config
          ] ++ x11Libs;
          SDK_ROOT = nrfSdk;
          LD_LIBRARY_PATH = lib.makeLibraryPath x11Libs;
          shellHook = ''
            # Cargo looks up subcommands (fmt, clippy) in $CARGO_HOME/bin before PATH;
            # a separate cargo home keeps the pinned toolchain's versions in charge.
            export CARGO_HOME=''${XDG_CACHE_HOME:-$HOME/.cache}/swet102/cargo
            echo "swet102 dev shell: cargo test · cargo run -p swet-emu · make · make check"
          '';
        };
      });
}
