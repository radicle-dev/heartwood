#! /usr/bin/env bash
set -e

CLUSTER_NAME=$1
PROVISIONER=$2

echo ""
echo -n "Are you sure you want to destroy the cluster and remove kubeconfig entries? [y/N] "
read -r answer
if [ "${answer:-N}" != "y" ]
then
    echo "Aborted."
    exit 1
fi

echo "${CHECK}Destroying talos cluster '$CLUSTER_NAME'...${NORMAL}"
if [ "$PROVISIONER" = "qemu" ]
then
    sudo --preserve-env=HOME,PATH talosctl cluster destroy --name "$CLUSTER_NAME" --provisioner "$PROVISIONER"
else
    talosctl cluster destroy --name "$CLUSTER_NAME" --provisioner "$PROVISIONER"
fi

echo "${CHECK}Removing kube config entries...${NORMAL}"
CONTEXT=$(kubectl config current-context 2>/dev/null || echo "")
if [ -n "$CONTEXT" ]
then
    CLUSTER=$(echo "$CONTEXT" | cut -d '@' -f 2)
    kubectl config delete-context "$CONTEXT" || true
    kubectl config delete-cluster "$CLUSTER" || true
    kubectl config unset "users.$CONTEXT" || true
fi
echo "${WARN}Make sure you remove the '$CLUSTER_NAME' entry from: ~/.talos/config${NORMAL}"
echo "${SUCCESS}Cluster destroyed.${NORMAL}"
