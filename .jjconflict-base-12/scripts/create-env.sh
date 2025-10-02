#!/bin/sh
set -e

unset SSH_AUTH_SOCK
unset SSH_AGENT_PID

tmp="$(mktemp -d)"

export RAD_HOME="$tmp/.radicle"
export RAD_PASSPHRASE=

set -x
mkdir "$tmp/acme"
cd "$tmp/acme"
echo "ACME" > README
echo "Copyright (c) 1978-1986 ACME Corp." > COPY

git init
git add README COPY
git commit -m "Initial commit" --no-gpg-sign

rad auth --alias alice
rad init --name "acme" --description "ACME Corp. Warez" --private --no-confirm

exec "$SHELL"
