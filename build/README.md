# Builds

Radicle uses a [reproducible build][rb] pipeline to make binary verification
easier and more secure.

[rb]: https://reproducible-builds.org/

This build pipeline is designed to be run on an x86_64 machine running Linux.
The output is a set of `.tar.xz` archives containing binaries for the supported
platforms and signed by the user's Radicle key.

These binaries are statically linked to be maximally portable, and designed to
be reproducible, byte for byte.

To run the build, simply enter the following command from the repository root:

    build/build.sh

This will build all targets and place the output in `build/artifacts` with
one sub-directory per build target.

Note that it will use `git describe` to get a version number for the build.
You *must* have a commit tagged with a version in your history or the build
will fail, eg. `v1.0.0`.

When the build completes, the SHA-256 checksums of the artifacts are output.
For a given Radicle version and source tree, the same set of checksums should
always be output, no matter where or when the build is run. If they do not
match, either the build pipeline has a bug, making it non-reproducible, or one
of the machines is compromised.

Here's an example output for a development version of Radicle:

    b9aa75bba175e18e05df4f6b39ec097414bbf56ccdeb4a2229b557f8b8e05404  radicle-1.0.0-rc.4-3-gb299f3b5-aarch64-apple-darwin.tar.xz
    c7070806bf2d17a8a0d3b329e4d57b1e544b7b82cb58e2863074d96348a2ab0d  radicle-1.0.0-rc.4-3-gb299f3b5-aarch64-unknown-linux-musl.tar.xz
    1a8327854f16ea90491fb90e0c3291a63c4b2ab01742c8435faec7d370cacb79  radicle-1.0.0-rc.4-3-gb299f3b5-x86_64-apple-darwin.tar.xz
    709ac67541ff0c0c570ac22ab2de9f98320e0cc2cc9b67f1909c014a2bb5bd49  radicle-1.0.0-rc.4-3-gb299f3b5-x86_64-unknown-linux-musl.tar.xz

A script is included in `build/checksums.sh` to output these checksums after
the artifacts are built.

## Requirements

The following software is required for the build:

  * `podman`
  * `rad` (The Radicle CLI)
  * `sha256sum`

## macOS

macOS binaries are not signed or notarized, so they have to be downloaded via
the CLI to avoid issues. A copy of a small subset of the Apple SDK is included
here to be able to cross-compile.

## Podman

We use `podman` to make the build reproducible on any machine by controlling
the build environment. We prefer `podman` to `docker` because it doesn't
require a background process to run and can be run without root access out of
the box.

The first time you run `podman`, you may have to give yourself some extra UIDs
for `podman` to use, with:

    sudo usermod --add-subuids 100000-165535 --add-subgids 100000-165535 $USER

Then update `podman` with:

    podman system migrate
