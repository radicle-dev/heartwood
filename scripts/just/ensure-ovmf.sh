#! /usr/bin/env bash
set -e
if [ -z "${OVMF_FD_PATH:-}" ]; then
    # Not on NixOS / not using the devshell — assume OVMF is installed normally
    exit 0
fi
if [ ! -f "/usr/share/OVMF/OVMF_CODE.fd" ]; then
    echo "${CHECK}Symlinking OVMF firmware from Nix store into /usr/share/OVMF...${NORMAL}"
    sudo mkdir -p /usr/share/OVMF
    sudo ln -sf "$OVMF_FD_PATH"/* /usr/share/OVMF/
fi
