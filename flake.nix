{
  description = "Panopticon Null: reproducible public-interest surveillance procurement evidence tooling";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.05";
    nixpkgs-unstable.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
    rustsec-advisory-db = {
      url = "github:RustSec/advisory-db";
      flake = false;
    };
  };

  outputs = { self, nixpkgs, nixpkgs-unstable, rust-overlay, rustsec-advisory-db }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
      perSystem = system:
        let
          overlays = [ (import rust-overlay) ];
          pkgs = import nixpkgs { inherit system overlays; };
          latestPkgs = import nixpkgs-unstable { inherit system; };
          rust = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
          rustPlatform = pkgs.makeRustPlatform { cargo = rust; rustc = rust; };
          nativeTools = with pkgs; [ pkg-config poppler_utils tesseract util-linux bubblewrap ];
          cargoDeny = latestPkgs.cargo-deny;
          cargoLock = { lockFile = ./Cargo.lock; };
          source = ./.;
          mkCargoCheck = name: command: rustPlatform.buildRustPackage {
            pname = "panopticon-null-${name}";
            version = "0.0.2";
            src = source;
            inherit cargoLock;
            nativeBuildInputs = nativeTools ++ [ cargoDeny pkgs.git ];
            buildPhase = ''
              runHook preBuild
              ${command}
              runHook postBuild
            '';
            installPhase = ''
              mkdir -p $out
              touch $out/passed
            '';
            doCheck = false;
          };
          package = rustPlatform.buildRustPackage {
            pname = "panopticon-null";
            version = "0.0.2";
            src = source;
            inherit cargoLock;
            nativeBuildInputs = nativeTools;
            cargoBuildFlags = [ "--workspace" "--all-features" ];
            checkPhase = ''
              runHook preCheck
              cargo test --workspace --all-features
              runHook postCheck
            '';
          };
        in {
          inherit pkgs rust package mkCargoCheck cargoDeny;
          advisoryDb = rustsec-advisory-db;
          nativeTools = nativeTools;
        };
    in {
      packages = forAllSystems (system: {
        default = (perSystem system).package;
        pnull = (perSystem system).package;
      });

      checks = forAllSystems (system:
        let
          value = perSystem system;
        in {
          build-and-test = value.package;
          formatting = value.mkCargoCheck "formatting" "cargo fmt --all --check";
          clippy = value.mkCargoCheck "clippy" "cargo clippy --workspace --all-targets --all-features -- -D warnings";
          dependency-policy = value.mkCargoCheck "dependency-policy" ''
            export CARGO_HOME=$TMPDIR/cargo-home
            mkdir -p "$CARGO_HOME/advisory-dbs"
            cp -r ${value.advisoryDb} "$TMPDIR/advisory-db"
            chmod -R u+w "$TMPDIR/advisory-db"
            git -C "$TMPDIR/advisory-db" init --quiet
            git -C "$TMPDIR/advisory-db" add .
            GIT_AUTHOR_DATE=2026-08-12T00:00:00Z GIT_COMMITTER_DATE=2026-08-12T00:00:00Z \
              git -C "$TMPDIR/advisory-db" -c user.name=RustSec -c user.email=noreply@localhost \
              commit --quiet --no-gpg-sign -m pinned
            ln -s "$TMPDIR/advisory-db" "$CARGO_HOME/advisory-dbs/advisory-db-3157b0e258782691"
            cargo deny --offline check
          '';
          offline-demo = value.pkgs.runCommand "panopticon-null-offline-demo" {
            nativeBuildInputs = value.nativeTools;
          } ''
            cp -r ${self} source
            chmod -R u+w source
            cd source
            ${value.package}/bin/pnull demo --output "$out"
            test "$(cat "$out/network-posts.txt")" = 0
            test -f "$out/site/index.html"
            test -f "$out/site/atom.xml"
          '';
        });

      devShells = forAllSystems (system:
        let
          value = perSystem system;
        in {
          default = value.pkgs.mkShell {
            packages = value.nativeTools ++ (with value.pkgs; [
              value.rust
              rust-analyzer
              value.cargoDeny
              sqlite
            ]);
            RUST_SRC_PATH = "${value.rust}/lib/rustlib/src/rust/library";
          };
        });
    };
}
