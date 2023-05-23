#!/bin/bash

set -o errexit
set -o nounset
set -o pipefail

main () {
    if [[ $# -ne 2 ]]; then
        echo "$#: Wrong number of arguments"
        return 1
    fi

    local target=$1
    local sha=$2

    echo "$SSH_PRIVATE_KEY" >ssh-private-key
    chmod go-rwx ssh-private-key
    ssh -o UserKnownHostsFile=/dev/null -o StrictHostKeyChecking=no -i ssh-private-key github-actions@files.radicle.xyz mkdir -p "/mnt/radicle/releases/$sha/$target/debug"
    scp -o UserKnownHostsFile=/dev/null -o StrictHostKeyChecking=no -i ssh-private-key -r radicle-$target/* "github-actions@files.radicle.xyz:/mnt/radicle/releases/$sha/$target"
}

main "$@"
