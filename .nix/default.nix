{ sources ? import ./sources.nix
, pkgs ? import sources.nixpkgs {
    overlays = [ (import sources.rust-overlay) ];
  }
, rust-overlay ? pkgs.rust-bin.stable.latest.default
}:
  with pkgs;
  mkShell {
    name = "build";
    buildInputs = [
        # cargo tooling
        cargo-deny
        cargo-watch

        # hard dependencies
        cmake
        openssl
        pkgconfig
        rust-overlay
    ];
  }
