{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    fenix.url = "github:nix-community/fenix/monthly";
    flake-utils.url = "github:numtide/flake-utils";
  };
  outputs = { self, nixpkgs, flake-utils, ... }@inputs: flake-utils.lib.eachDefaultSystem (system:
    let
      pkgs = import nixpkgs {
        inherit system;
        overlays = [ inputs.fenix.overlays.default ];
      };
    in
    {
      devShells.default = pkgs.mkShell rec {
        nativeBuildInputs = with pkgs; [
          (fenix.complete.withComponents [
            "rustc"
            "cargo"
            "clippy"
            "rustfmt"
            "rust-analyzer"
            "miri"
            "rust-src"
            "rustc-codegen-cranelift-preview"
            "llvm-tools-preview"
          ])
          fenix.targets.x86_64-unknown-linux-gnu.latest.rust-std

          cargo-msrv
          cargo-wizard
          
          sccache
          lld
          mold

          fuse
          openssl.dev
          pkg-config
        ];
        RUST_SRC_PATH = "${pkgs.fenix.complete.rust-src}/lib/rustlib/src/rust/library";
        RUSTC_WRAPPER="sccache";
        RUSTFLAGS="-Zthreads=12 -Ctarget-cpu=native -Clink-arg=-fuse-ld=mold";
        MSRVFLAGS="-Clink-arg=-fuse-ld=mold"; # RUSTFLAGS=$MSRVFLAGS cargo msrv

        LD_LIBRARY_PATH = with pkgs; lib.makeLibraryPath nativeBuildInputs;
      };
    });
}
