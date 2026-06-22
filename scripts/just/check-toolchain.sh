#! /usr/bin/env bash
set -e

dockerfile="build/Dockerfile"
dockerfile_version="$(sed -nE 's/^ARG RUST_VERSION="([1-9]\.[0-9]+)".*/\1/p' $dockerfile)"
rust_toolchain_version="$(rustc --version | sed -nE 's/rustc ([1-9]\.[0-9]+)\.[0-9]+.*/\1/p')"

if [ "$dockerfile_version" != "$rust_toolchain_version" ]; then
    printf "${ERROR}Rust version mismatch: ./$dockerfile=%s, rustc=%s${NORMAL}\n" "$dockerfile_version" "$rust_toolchain_version";
    exit 1;
fi
