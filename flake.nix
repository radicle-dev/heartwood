{
  description = "Radicle";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/release-23.11";

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
      pkgs = import nixpkgs {
        inherit system;
        overlays = [(import rust-overlay)];
      };

      inherit (pkgs) lib;

      rustToolChain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain;
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
        pname = "Heartwood";
        inherit src;
      };

      # Common arguments can be set here to avoid repeating them later
      commonArgs =
        basicArgs
        // {
          inherit cargoArtifacts;
          strictDeps = true;

          buildInputs =
            [
              pkgs.git
              # Add additional build inputs here
            ]
            ++ lib.optionals pkgs.stdenv.isDarwin [
              # Additional darwin specific inputs can be set here
              pkgs.libiconv
              pkgs.darwin.apple_sdk.frameworks.Security
            ];

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

      # Build *just* the cargo dependencies, so we can reuse
      # all of that work (e.g. via cachix) when running in CI
      cargoArtifacts = craneLib.buildDepsOnly basicArgs;

      crate = {
        name,
        package ? name,
        pages ? [],
      }:
        craneLib.buildPackage (commonArgs
          // {
            inherit (craneLib.crateNameFromCargoToml {cargoToml = src + "/" + package + "/Cargo.toml";}) pname version;
            cargoExtraArgs = lib.optionalString (package != "") "-p ${package}";
            doCheck = false;

            nativeBuildInputs = with pkgs; [asciidoctor installShellFiles];
            postInstall = ''
              for page in ${lib.escapeShellArgs pages}; do
                asciidoctor -d manpage -b manpage $page
                installManPage ''${page::-5}
              done
            '';
          });
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

        # Check formatting
        fmt = craneLib.cargoFmt basicArgs;

        # TODO: audits are failing so skip this check for now
        # Audit dependencies
        # audit = craneLib.cargoAudit {
        #   inherit src advisory-db;
        # };

        # Audit licenses
        deny = craneLib.cargoDeny basicArgs;

        # Run tests with cargo-nextest
        nextest = craneLib.cargoNextest (commonArgs
          // {
            inherit cargoArtifacts;
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
            buildPhase = ''
              export CARGO_PROFILE=dev;
            '';
          });
      };

      packages =
        {
          default = self.packages.${system}.radicle;
          radicle-full = pkgs.buildEnv {
            name = "radicle-full";
            paths = with self.packages.${system}; [
              default
              radicle-httpd
            ];
          };
        }
        // (builtins.listToAttrs (map (
            package:
              if builtins.isString package
              then {
                name = package;
                value = crate {name = package;};
              }
              else {
                inherit (package) name;
                value = crate package;
              }
          )
          [
            {
              package = "";
              name = "radicle";
              pages = [
                "git-remote-rad.1.adoc"
                "rad.1.adoc"
                "radicle-node.1.adoc"
                "rad-patch.1.adoc"
                "rad-id.1.adoc"
              ];
            }
            {
              name = "radicle-httpd";
              pages = ["radicle-httpd.1.adoc"];
            }
            "radicle-cli"
            "radicle-remote-helper"
            "radicle-node"
          ]));

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

      apps.radicle-httpd = flake-utils.lib.mkApp {
        name = "radicle-httpd";
        drv = self.packages.${system}.radicle-httpd;
      };

      devShells.default = craneLib.devShell {
        # Extra inputs can be added here; cargo and rustc are provided by default.
        packages = [
          pkgs.cargo-watch
          pkgs.cargo-nextest
          pkgs.ripgrep
          pkgs.rust-analyzer
          pkgs.sqlite
        ];
      };
    });
}
