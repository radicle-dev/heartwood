#!/bin/sh

set -e

if [ $# -lt 2 ]; then
  echo "usage: $0 <output-dir> <input-file>..."
  exit 1
fi

outdir="$1"
shift

for input in "$@"; do
  echo "Building $input.."
  asciidoctor --doctype manpage --backend manpage --destination-dir "$outdir" "$input"
done
