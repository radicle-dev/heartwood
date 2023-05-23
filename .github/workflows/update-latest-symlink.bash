#!/bin/bash

set -o errexit
set -o nounset
set -o pipefail

main () {
    if [[ $# -ne 1 ]]; then
        echo "$#: Wrong number of arguments"
        return 1
    fi

    local sha=$1
    echo "$SSH_PRIVATE_KEY" >ssh-private-key
    chmod go-rwx ssh-private-key
    ssh -o UserKnownHostsFile=/dev/null -o StrictHostKeyChecking=no -i ssh-private-key github-actions@files.radicle.xyz ln -snf "/mnt/radicle/releases/${sha}" "/mnt/radicle/releases/latest"
}

main "$@"
