#!/bin/sh

set -e

# Attempt to install `asciidoctor` on Debian, Arch Linux and MacOS.
install() {
  os="$(uname)"

  case "$os" in
    Linux)
      if command -v apt-get >/dev/null 2>&1; then
        # Debian
        apt-get update
        apt-get install -y asciidoctor
      elif command -v pacman >/dev/null 2>&1; then
        # Arch Linux
        pacman -Sy --noconfirm asciidoctor
      fi ;;
    Darwin) # MacOS
      if command -v brew >/dev/null 2>&1; then
        brew install asciidoctor
      fi ;;
    *)
      echo "fatal: unknown operating system: $os"
      exit 1 ;;
  esac
}

main () {
    if [ $# -ne 1 ]; then
        echo "$#: Wrong number of arguments"
        exit 1
    fi

    if ! command -v asciidoctor >/dev/null 2>&1; then
      install
    fi

    target="$1"
    rustup target add "$target"

    staging="radicle-$target"
    mkdir -p "$staging"

    cargo build --target="$target" --package=radicle-httpd --release
    cp target/"$target"/release/radicle-httpd "$staging"/

    cargo build --target="$target" --package=radicle-node --release
    cp target/"$target"/release/radicle-node "$staging"/

    cargo build --target="$target" --bin rad --release
    cp target/"$target"/release/rad "$staging"/

    cargo build --target="$target" --bin git-remote-rad --release
    cp target/"$target"/release/git-remote-rad "$staging"/

    scripts/build-man-pages.sh "$staging" "$(find . -name '*.1.adoc')"

    tar czf "$staging.tar.gz" "$staging"
    cp "$staging.tar.gz" "$staging"/
}

main "$@"
