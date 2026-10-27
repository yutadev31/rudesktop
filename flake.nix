{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      rust-overlay,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };

        rust = pkgs.rust-bin.stable.latest.default.override {
          extensions = [
            "rust-src"
            "rust-analyzer"
          ];
        };

        devTools = with pkgs; [
          rust
          tombi
          nixd
          nixfmt
          typos-lsp
        ];

        nativeBuildInputs = with pkgs; [
          pkg-config
        ];

        buildInputs = with pkgs; [
          libpulseaudio
          libxkbcommon
          wayland
        ];

        lockfile = builtins.fromTOML (builtins.readFile ./Cargo.lock);
        cpstPackage = builtins.head (builtins.filter (package: package.name == "cpst") lockfile.package);
        rubarPackage = builtins.head (builtins.filter (package: package.name == "rubar") lockfile.package);
        rumenuPackage = builtins.head (builtins.filter (package: package.name == "rumenu") lockfile.package);
        shotPackage = builtins.head (builtins.filter (package: package.name == "shot") lockfile.package);

        cpst = pkgs.rustPlatform.buildRustPackage {
          pname = "cpst";
          version = cpstPackage.version;
          src = ./.;

          cargoLock.lockFile = ./Cargo.lock;
          cargoBuildFlags = [
            "--package"
            "cpst"
          ];
          cargoTestFlags = [
            "--package"
            "cpst"
          ];

          inherit nativeBuildInputs buildInputs;
        };

        rubar = pkgs.rustPlatform.buildRustPackage {
          pname = "rubar";
          version = rubarPackage.version;
          src = ./.;

          cargoLock.lockFile = ./Cargo.lock;
          cargoBuildFlags = [
            "--package"
            "rubar"
          ];
          cargoTestFlags = [
            "--package"
            "rubar"
          ];

          inherit nativeBuildInputs buildInputs;

          preCheck = ''
            export FONTCONFIG_FILE="${
              pkgs.makeFontsConf {
                fontDirectories = [ pkgs.dejavu_fonts ];
              }
            }"
          '';
        };

        rumenu = pkgs.rustPlatform.buildRustPackage {
          pname = "rumenu";
          version = rumenuPackage.version;
          src = ./.;

          cargoLock.lockFile = ./Cargo.lock;
          cargoBuildFlags = [ "--package" "rumenu" ];
          cargoTestFlags = [ "--package" "rumenu" ];

          inherit nativeBuildInputs buildInputs;
        };

        shot = pkgs.rustPlatform.buildRustPackage {
          pname = "shot";
          version = shotPackage.version;
          src = ./.;

          cargoLock.lockFile = ./Cargo.lock;
          cargoBuildFlags = [
            "--package"
            "shot"
          ];
          cargoTestFlags = [
            "--package"
            "shot"
          ];

          inherit nativeBuildInputs buildInputs;
        };
      in
      {
        packages = {
          inherit cpst rubar rumenu shot;
          default = rubar;
        };

        devShells.default = pkgs.mkShell {
          nativeBuildInputs = nativeBuildInputs ++ devTools;
          inherit buildInputs;
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath buildInputs;
        };
      }
    );
}
