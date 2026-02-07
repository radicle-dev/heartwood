{
  description = "Radicle";

  inputs = {
    nixpkgs-unstable.url = "github:NixOS/nixpkgs/release-25.05";
    nixpkgs-stable.url = "github:NixOS/nixpkgs/release-25.05";
    nixpkgs.follows = "nixpkgs-stable";

    crane.url = "github:ipetkov/crane";

    git-hooks = {
      url = "github:cachix/git-hooks.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    flake-utils.url = "github:numtide/flake-utils";

    advisory-db = {
      url = "github:rustsec/advisory-db";
      flake = false;
    };
  };

  nixConfig = {
    keepOutputs = true;
    extra-substituters = ["https://attic.radicle.xyz/radicle"];
    extra-trusted-public-keys = ["radicle:TruHbueGHPm9iYSq7Gq6wJApJOqddWH+CEo+fsZnf4g="];
  };

  outputs = {
    self,
    nixpkgs,
    crane,
    flake-utils,
    advisory-db,
    rust-overlay,
    ...
  } @ inputs:
    flake-utils.lib.eachDefaultSystem (system: let
      lib = nixpkgs.lib;
      pkgs = import nixpkgs {
        inherit system;
        overlays = [(import rust-overlay)];
      };

      msrv = let
        msrv = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).workspace.package.rust-version;
      in rec {
        toolchain = pkgs.rust-bin.stable.${msrv}.default;
        craneLib = (crane.mkLib pkgs).overrideToolchain toolchain;
        commonArgs = mkCommonArgs craneLib;
      };

      rustup = rec {
        toolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        craneLib = (crane.mkLib pkgs).overrideToolchain toolchain;
        commonArgs = mkCommonArgs craneLib;
      };

      rustupDevShell = rec {
        toolchain = rustup.toolchain.override (prev: {
          extensions = prev.extensions ++ ["rust-analyzer"];
        });
        craneLib = (crane.mkLib pkgs).overrideToolchain toolchain;
        commonArgs = mkCommonArgs craneLib;
      };

      srcFilters = path: type:
        builtins.any (suffix: lib.hasSuffix suffix path) [
          ".sql" # schemas
          ".diff" # testing
          ".md" # testing
          ".adoc" # man pages
          ".json" # testing samples
          ".txt" # might be included with `include_str!`
          "rad-cob-multiset" # testing external COBs
        ]
        ||
        # Default filter from crane (allow .rs files)
        (rustup.craneLib.filterCargoSources path type);

      src = lib.cleanSourceWith {
        src = ./.;
        filter = srcFilters;
      };

      basicArgs = {
        inherit src;
        pname = "Heartwood";
        strictDeps = true;
      };

      # Common arguments can be set here to avoid repeating them later
      mkCommonArgs = craneLib:
        basicArgs
        // {
          # Build *just* the cargo dependencies, so we can reuse
          # all of that work (e.g. via cachix) when running in CI
          cargoArtifacts = craneLib.buildDepsOnly basicArgs;

          nativeBuildInputs = with pkgs; [
            asciidoctor
            git
            installShellFiles
          ];
          buildInputs = lib.optionals pkgs.stdenv.buildPlatform.isDarwin (with pkgs; [
            darwin.apple_sdk.frameworks.Security
          ]);
          nativeCheckInputs = with pkgs; [
            jq
            jujutsu
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

      buildCrate = rust: {
        name,
        pages ? [],
      }:
        rust.craneLib.buildPackage (rust.commonArgs
          // {
            inherit (rust.craneLib.crateNameFromCargoToml {cargoToml = src + "/crates/" + name + "/Cargo.toml";}) pname version;
            cargoExtraArgs = "-p ${name}";
            doCheck = false;
            postInstall = ''
              for page in ${lib.escapeShellArgs pages}; do
                asciidoctor -d manpage -b manpage $page
                installManPage ''${page::-5}
              done
            '';
          });
      buildCrates = {
        rust ? rustup,
        prefix ? "",
      }:
        builtins.listToAttrs (map
          ({name, ...} @ package: lib.nameValuePair (prefix + name) ((buildCrate rust) package))
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
    in {
      # Formatter
      formatter = pkgs.alejandra;

      # Set of checks that are run: `nix flake check`
      checks =
        (buildCrates {
          rust = msrv;
          prefix = "msrv-";
        })
        // {
          pre-commit-check = let
            grep = rec {
              generators = [
                {
                  word = "radicle.xyz";
                  files = "\\.rs$";
                  excludes = [];
                }
                {
                  word = "radicle.zulipchat.com";
                  files = "\\.rs$";
                  excludes = [];
                }
                {
                  word = "git2::";
                  files = "^crates/radicle/.*\\.rs$";
                  excludes = ["crates/radicle/src/git/raw.rs"];
                }
              ];
              after = map id generators;
              prefix = "grep-";
              id = {word, ...}: prefix + word;
              hooks = builtins.listToAttrs (map (generator: {
                  # "," is problematic, as this is used to split
                  # lists of hook names, when skipping, see:
                  # https://pre-commit.com/#temporarily-disabling-hooks
                  name = assert !lib.hasInfix "," generator.word; id generator;
                  value = hook generator;
                })
                generators);
              hook = {
                word,
                files,
                excludes,
              }: {
                inherit files excludes;
                enable = true;
                entry = builtins.toString (pkgs.writeShellScript
                  "grep-${word}"
                  "! ${lib.getExe pkgs.ripgrep} --context=3 --fixed-strings '${word}' $@");
                name = "Avoid '${word}' in '${files}'";
                pass_filenames = true;
              };
            };
          in
            inputs.git-hooks.lib.${system}.run {
              src = ./.;
              settings.rust.check.cargoDeps = pkgs.rustPlatform.importCargoLock {lockFile = ./Cargo.lock;};
              default_stages = [
                "pre-commit"
                "pre-push"
              ];
              hooks =
                {
                  alejandra.enable = true;
                  codespell = {
                    enable = true;
                    entry = "${lib.getExe pkgs.codespell} -w";
                    types = ["text"];
                  };
                  rustfmt = {
                    enable = true;
                    fail_fast = true;
                    packageOverrides.rustfmt = rustup.toolchain;
                  };
                  cargo-check = {
                    enable = true;
                    name = "cargo check";
                    after = ["rustfmt"] ++ grep.after;
                    fail_fast = true;
                  };
                  cargo-doc = let
                    # We wrap `cargo` in order to set an environment variable that
                    # gives us a non-zero exit on warning.
                    command =
                      pkgs.writeShellScript
                      "cargo"
                      "RUSTDOCFLAGS='--deny warnings' ${lib.getExe' rustup.toolchain "cargo"} $@";
                  in {
                    enable = true;
                    name = "cargo doc";
                    after = ["rustfmt"] ++ grep.after;
                    fail_fast = true;
                    entry = "${command} doc --workspace --all-features --no-deps";
                    files = "\\.rs$";
                    pass_filenames = false;
                  };
                  clippy = {
                    enable = true;
                    name = "cargo clippy";
                    stages = ["pre-push"]; # Only pre-push, because it takes a while.
                    settings = {
                      allFeatures = true;
                      denyWarnings = true;
                    };
                    packageOverrides = {
                      cargo = rustup.toolchain;
                      clippy = rustup.toolchain;
                    };
                  };
                  shellcheck.enable = true;
                }
                // grep.hooks;
            };

          # Build the crate as part of `nix flake check` for convenience
          inherit (self.packages.${system}) radicle;

          # Run clippy (and deny all warnings) on the crate source,
          # again, reusing the dependency artifacts from above.
          #
          # Note that this is done as a separate derivation so that
          # we can block the CI if there are issues here, but not
          # prevent downstream consumers from building our crate by itself.
          clippy = rustup.craneLib.cargoClippy (rustup.commonArgs
            // {
              cargoClippyExtraArgs = "--all-targets -- --deny warnings";
            });

          doc = rustup.craneLib.cargoDoc rustup.commonArgs;
          deny = rustup.craneLib.cargoDeny rustup.commonArgs;
          fmt = rustup.craneLib.cargoFmt basicArgs;

          audit = rustup.craneLib.cargoAudit {
            inherit src advisory-db;
          };

          # Run tests with cargo-nextest
          nextest = rustup.craneLib.cargoNextest (rustup.commonArgs
            // {
              # Ensure that the binaries are built for the radicle-cli tests to
              # avoid timeouts
              preCheck = ''
                patchShebangs --build radicle-cli/examples/rad-cob-multiset
                cargo build -p radicle-remote-helper --target-dir radicle-cli/target
                cargo build -p radicle-cli --target-dir radicle-cli/target
              '';
              # Ensure dev is used since we rely on env variables being
              # set in tests.
              env.CARGO_PROFILE = "dev";
              cargoNextestExtraArgs = "--no-capture";
            });
        };

      packages = let
        crates = buildCrates {};
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

      devShells.default = rustupDevShell.craneLib.devShell {
        inherit (self.checks.${system}.pre-commit-check) shellHook;
        buildInputs = self.checks.${system}.pre-commit-check.enabledPackages;

        # Extra inputs can be added here; cargo and rustc are provided by default.
        packages = with pkgs; [
          cargo-audit
          cargo-deny
          cargo-watch
          cargo-nextest
          cargo-semver-checks
          ripgrep
          sqlite
        ];

        env.RUST_SRC_PATH = "${rustupDevShell.toolchain}/lib/rustlib/src/rust/library";
      };
    });
}
