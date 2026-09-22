{
  description = "A zellij tab-bar plugin that wraps tabs across multiple rows";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    # nixpkgs' own rustc ships std for wasm32-unknown-unknown and wasm32v1-none
    # but NOT wasm32-wasip1, which is what zellij plugins target. fenix supplies
    # that std for an otherwise ordinary native toolchain. See "Why fenix" in
    # README.md for why pkgsCross.wasi32 doesn't work here.
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, fenix }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forAllSystems = f:
        nixpkgs.lib.genAttrs systems (system: f system nixpkgs.legacyPackages.${system});

      mkPlugin = system: pkgs:
        let
          toolchain = with fenix.packages.${system}; combine [
            stable.rustc
            stable.cargo
            targets.wasm32-wasip1.stable.rust-std
          ];
          rustPlatform = pkgs.makeRustPlatform {
            cargo = toolchain;
            rustc = toolchain;
          };
        in
        rustPlatform.buildRustPackage {
          pname = "zellij-multi-row-tab-bar";
          version = "0.1.0";

          src = self;
          cargoLock.lockFile = ./Cargo.lock;

          # The wasm artifact needs neither of these, but crates in
          # zellij-utils' dependency tree run build scripts that compile for the
          # host and look for openssl via pkg-config.
          nativeBuildInputs = [ pkgs.pkg-config ];
          buildInputs = [ pkgs.openssl ];

          # A zellij plugin is a wasm module: no tests to run, and nothing for
          # the default installPhase to find.
          doCheck = false;
          auditable = false;

          # buildRustPackage's build hook targets the host unless this is a
          # cross build, so drive cargo directly. cargoSetupHook has already
          # vendored the dependencies and put cargo in offline mode.
          buildPhase = ''
            runHook preBuild
            cargo build --release --offline --target wasm32-wasip1
            runHook postBuild
          '';

          installPhase = ''
            runHook preInstall
            install -Dm444 target/wasm32-wasip1/release/multi-row-tab-bar.wasm \
              $out/share/zellij/plugins/multi-row-tab-bar.wasm
            runHook postInstall
          '';

          meta = with nixpkgs.lib; {
            description = "A zellij tab-bar plugin that wraps tabs across multiple rows";
            homepage = "https://github.com/brubsby/zellij-multi-row-tab-bar";
            license = licenses.mit;
            platforms = systems;
          };
        };
    in
    {
      packages = forAllSystems (system: pkgs: rec {
        zellij-multi-row-tab-bar = mkPlugin system pkgs;
        default = zellij-multi-row-tab-bar;
      });

      overlays.default = final: _prev: {
        zellij-multi-row-tab-bar = mkPlugin final.stdenv.hostPlatform.system final;
      };
    };
}
