{
  description = "Radicle";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/release-24.05";

    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs = {
        nixpkgs.follows = "nixpkgs";
        flake-utils.follows = "flake-utils";
      };
    };

    flake-utils.url = "github:numtide/flake-utils";

    advisory-db = {
      url = "github:rustsec/advisory-db";
      flake = false;
    };
  };

  nixConfig = {
    keepOutputs = true;
  };

  outputs = {
    self,
    nixpkgs,
    crane,
    flake-utils,
    advisory-db,
    rust-overlay,
    ...
  }:
    flake-utils.lib.eachDefaultSystem (system: let
      lib = nixpkgs.lib;
      pkgs = import nixpkgs {
        inherit system;
        overlays = [(import rust-overlay)];
      };

      rustToolChain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
      craneLib = (crane.mkLib pkgs).overrideToolchain rustToolChain;

      srcFilters = path: type:
        builtins.any (suffix: lib.hasSuffix suffix path) [
          ".sql" # schemas
          ".diff" # testing
          ".md" # testing
          ".adoc" # man pages
        ]
        ||
        # Default filter from crane (allow .rs files)
        (craneLib.filterCargoSources path type);

      src = lib.cleanSourceWith {
        src = ./.;
        filter = srcFilters;
      };

      basicArgs = {
        inherit src;
        pname = "Heartwood";
        strictDeps = true;
      };

      # Build *just* the cargo dependencies, so we can reuse
      # all of that work (e.g. via cachix) when running in CI
      cargoArtifacts = craneLib.buildDepsOnly basicArgs;

      # Common arguments can be set here to avoid repeating them later
      commonArgs =
        basicArgs
        // {
          inherit cargoArtifacts;

          nativeBuildInputs = with pkgs;
            [
              git
              # Add additional build inputs here
            ]
            ++ lib.optionals pkgs.stdenv.isDarwin (with pkgs; [
              # Additional darwin specific inputs can be set here
              libiconv
              darwin.apple_sdk.frameworks.Security
            ]);

          env =
            {
              RADICLE_VERSION = "nix-" + (self.shortRev or self.dirtyShortRev or "unknown");
            }
            // (
              if self ? rev || self ? dirtyRev
              then {
                GIT_HEAD = self.rev or self.dirtyRev;
              }
              else {}
            );
        };
    in {
      # Formatter
      formatter = pkgs.alejandra;

      # Set of checks that are run: `nix flake check`
      checks = {
        # Build the crate as part of `nix flake check` for convenience
        inherit (self.packages.${system}) radicle;

        # Run clippy (and deny all warnings) on the crate source,
        # again, reusing the dependency artifacts from above.
        #
        # Note that this is done as a separate derivation so that
        # we can block the CI if there are issues here, but not
        # prevent downstream consumers from building our crate by itself.
        clippy = craneLib.cargoClippy (commonArgs
          // {
            cargoClippyExtraArgs = "--all-targets -- --deny warnings";
          });

        doc = craneLib.cargoDoc commonArgs;
        deny = craneLib.cargoDeny commonArgs;
        fmt = craneLib.cargoFmt basicArgs;

        audit = craneLib.cargoAudit {
          inherit src advisory-db;
        };

        # Run tests with cargo-nextest
        nextest = craneLib.cargoNextest (commonArgs
          // {
            partitions = 1;
            partitionType = "count";
            nativeBuildInputs = [
              # git is required so the sandbox can access it.
              pkgs.git
              # Ensure that `git-remote-rad` is present for testing.
              self.packages.${system}.radicle-remote-helper
            ];
            # Ensure dev is used since we rely on env variables being
            # set in tests.
            env.CARGO_PROFILE = "dev";
          });
      };

      packages = let
        crate = {
          name,
          pages ? [],
        }:
          craneLib.buildPackage (commonArgs
            // {
              inherit (craneLib.crateNameFromCargoToml {cargoToml = src + "/" + name + "/Cargo.toml";}) pname version;
              cargoExtraArgs = "-p ${name}";
              doCheck = false;

              nativeBuildInputs = with pkgs; [asciidoctor installShellFiles jq];
              postInstall = ''
                for page in ${lib.escapeShellArgs pages}; do
                  asciidoctor -d manpage -b manpage $page
                  installManPage ''${page::-5}
                done
              '';
            });
        crates = builtins.listToAttrs (map
          ({name, ...} @ package: lib.nameValuePair name (crate package))
          [
            {
              name = "radicle-cli";
              pages = [
                "rad.1.adoc"
                "rad-id.1.adoc"
                "rad-patch.1.adoc"
              ];
            }
            {
              name = "radicle-remote-helper";
              pages = ["git-remote-rad.1.adoc"];
            }
            {
              name = "radicle-node";
              pages = ["radicle-node.1.adoc"];
            }
          ]);
      in
        crates
        // rec {
          default = radicle;
          radicle = pkgs.buildEnv {
            name = "radicle";
            paths = with crates; [
              radicle-cli
              radicle-node
              radicle-remote-helper
            ];
          };
          radicle-full = pkgs.buildEnv {
            name = "radicle-full";
            paths = builtins.attrValues crates;
          };
        };

      apps.default = flake-utils.lib.mkApp {
        drv = self.packages.${system}.radicle;
      };

      apps.radicle-full = flake-utils.lib.mkApp {
        name = "rad";
        drv = self.packages.${system}.radicle-full;
      };

      apps.rad = flake-utils.lib.mkApp {
        name = "rad";
        drv = self.packages.${system}.radicle-cli;
      };

      apps.git-remote-rad = flake-utils.lib.mkApp {
        name = "git-remote-rad";
        drv = self.packages.${system}.radicle-remote-helper;
      };

      apps.radicle-node = flake-utils.lib.mkApp {
        name = "radicle-node";
        drv = self.packages.${system}.radicle-node;
      };

      devShells.default = craneLib.devShell {
        # Extra inputs can be added here; cargo and rustc are provided by default.
        packages = with pkgs; [
          cargo-watch
          cargo-nextest
          ripgrep
          rust-analyzer
          sqlite
        ];

        env.RUST_SRC_PATH = "${rustToolChain}/lib/rustlib/src/rust/library";
      };
    });
}
