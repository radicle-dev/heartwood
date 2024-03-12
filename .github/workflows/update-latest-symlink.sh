#!/bin/sh

set -o errexit
set -o nounset

remotely() {
  ssh -o UserKnownHostsFile=/dev/null -o StrictHostKeyChecking=no -i ssh-private-key github-actions@files.radicle.xyz "$@"
}

main () {
  if [ $# -ne 1 ]; then
    echo "$#: Wrong number of arguments"
    return 1
  fi
  target="x86_64-unknown-linux-musl"
  sha=$1

  trap 'rm -f ssh-private-key' EXIT

  echo "$SSH_PRIVATE_KEY" > ssh-private-key
  chmod go-rwx ssh-private-key

  remotely "/mnt/radicle/releases/${sha}/${target}/rad version --json > /mnt/radicle/releases/${sha}/version.json"
  remotely ln -snf "/mnt/radicle/releases/${sha}" "/mnt/radicle/releases/latest"
}

main "$@"
