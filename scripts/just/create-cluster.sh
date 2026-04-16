#! /usr/bin/env bash
set -e

CLUSTERS_DIR=$1
CLUSTER_NAME=$2
PROVISIONER=$3

if [ ! -d "$CLUSTERS_DIR/$CLUSTER_NAME" ]
then
    echo "${CHECK}Creating Talos cluster '$CLUSTER_NAME' using $PROVISIONER...${NORMAL}"
    mkdir -p "$CLUSTERS_DIR"
    if [ "$PROVISIONER" = "qemu" ]; then
        sudo --preserve-env=HOME,PATH talosctl cluster create --name="$CLUSTER_NAME" "$PROVISIONER" --config-patch-controlplanes '{"cluster": {"allowSchedulingOnControlPlanes": true}}'
    else
        talosctl cluster create --name="$CLUSTER_NAME" "$PROVISIONER" --config-patch-controlplanes '{"cluster": {"allowSchedulingOnControlPlanes": true}}'
    fi
else
    echo "${SUCCESS}Cluster '$CLUSTER_NAME' already exists.${NORMAL}"
fi
