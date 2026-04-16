#! /usr/bin/env bash

if command -v alejandra >/dev/null 2>&1; then
    alejandra --check .
else
    echo "⏭️ alejandra not found, skipping Nix formatting."
fi
