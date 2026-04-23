#! /usr/bin/env bash
set -e

RADICLE_NODE_MODULE=$1
MODULE_PKG=$2
MODULE_GEN=$3

cd "$RADICLE_NODE_MODULE"
if [ ! -d "$MODULE_PKG" ]
then
    echo "${CHECK}Fetching Timoni pkg files...${NORMAL}"
    timoni artifact pull oci://ghcr.io/stefanprodan/timoni/schemas -o cue.mod/pkg
fi
if [ ! -d "$MODULE_GEN" ]
then
    echo "${CHECK}Fetching Timoni k8s gen files...${NORMAL}"
    timoni mod vendor k8s
fi
