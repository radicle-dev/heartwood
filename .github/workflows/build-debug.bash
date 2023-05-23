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
    staging="radicle-$target"
    mkdir -p "$staging/debug"

    cargo build --target "$target" --bin git-remote-rad
    cp target/"$target"/debug/git-remote-rad "$staging"/debug/

    cargo build --target="$target" --bin rad
    cp target/"$target"/debug/rad "$staging"/debug/

    tar czf "$staging-debug.tar.gz" "$staging"/debug
    cp "$staging-debug.tar.gz" "$staging"/debug/
}

main "$@"
