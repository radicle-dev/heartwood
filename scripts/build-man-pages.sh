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

main() {
  if [ $# -lt 2 ]; then
    echo "usage: $0 <output-dir> <input-file>..."
    exit 1
  fi

  outdir="$1"
  shift

  if ! command -v asciidoctor >/dev/null 2>&1; then
    echo "Installing 'asciidoctor'.."
    install
  fi

  for input in "$@"; do
    echo "Building $input.."
    asciidoctor --doctype manpage --backend manpage --destination-dir "$outdir" "$input"
  done
}

main "$@"
