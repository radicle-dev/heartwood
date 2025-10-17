#! /usr/bin/env bash
set -e
TOOL=$1
PKG_NAME=$2

if ! command -v "$TOOL" >/dev/null 2>&1; then
    PKG="${PKG_NAME:-$TOOL}"
    echo "${ERROR}Missing required tool: ${TOOL}${NORMAL}"
    echo "${HINT}Use your systems package manager to install '$PKG'.${NORMAL}"
    exit 1
fi
