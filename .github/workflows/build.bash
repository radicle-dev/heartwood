#!/bin/bash

set -o errexit
set -o nounset
set -o pipefail

main () {
    if [[ $# -ne 1 ]]; then
        echo "$#: Wrong number of arguments"
        return 1
    fi

    local target=$1
    rustup target add "$target"

    local staging="radicle-$target"
    mkdir -p "$staging"

    cargo build --target="$target" --package=radicle-httpd --release
    cp target/"$target"/release/radicle-httpd "$staging"/

    cargo build --target="$target" --package=radicle-node --release
    cp target/"$target"/release/radicle-node "$staging"/

    cargo build --target="$target" --bin rad --release
    cp target/"$target"/release/rad "$staging"/

    cargo build --target="$target" --bin git-remote-rad --release
    cp target/"$target"/release/git-remote-rad "$staging"/

    ./build-man-pages.sh "$staging" "$(find . -name '*.1.adoc')"

    tar czf "$staging.tar.gz" "$staging"
    cp "$staging.tar.gz" "$staging"/
}

main "$@"
