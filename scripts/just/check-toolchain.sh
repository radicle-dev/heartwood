#! /usr/bin/env bash
set -e

dockerfile="build/Dockerfile"
dockerfile_version=$(dockerfile-json $dockerfile | jq -r '.MetaArgs[] | select(.Key=="RUST_VERSION") | .DefaultValue | ltrimstr("\"") | rtrimstr("\"")')
rust_toolchain_version="$(rustc --version | grep -Eo '([1-9]\.[0-9]+)\.[0-9]+')"

if [ "$dockerfile_version" != "$rust_toolchain_version" ]; then
    printf "${ERROR}Rust version mismatch: ./$dockerfile=%s, rustc=%s${NORMAL}\n" "$dockerfile_version" "$rust_toolchain_version";
    exit 1;
fi
