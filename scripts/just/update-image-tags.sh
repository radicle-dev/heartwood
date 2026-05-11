#! /usr/bin/env bash
set -e

VALUES_FILE="$1"

if [ -z "$VALUES_FILE" ]; then
  echo "${ERROR}Usage: $0 <path-to-values.cue>${NORMAL}" >&2
  exit 1
fi

REPO="radicle_garden/radicle-node"
API_URL="https://quay.io/api/v1/repository/$REPO/tag/?limit=100"

if [ ! -f "$VALUES_FILE" ]; then
  echo "${ERROR}Error: $VALUES_FILE not found.${NORMAL}" >&2
  exit 1
fi

echo "${CHECK}Fetching tags from quay.io for $REPO...${NORMAL}"

# Fetch tags, extract names, ignore empty lines, sort versions, and remove duplicates
TAGS=$(curl -sL "$API_URL" | jq -r '.tags[].name' | grep -v '^$' | sort -V | uniq)

if [ -z "$TAGS" ]; then
  echo "${ERROR}Error: No tags found or failed to fetch tags.${NORMAL}" >&2
  exit 1
fi

# Format tags into a CUE enum: "tag1" | "tag2" | string | *"latest"
# We append `| string | *"latest"` to allow custom local builds
ENUM=$(echo "$TAGS" | awk '{printf "\"%s\" | ", $0} END {print "string | *\"latest\""}')

echo "${CHECK}Injecting tags into $VALUES_FILE...${NORMAL}"

# Use a temporary file for cross-platform sed compatibility (works on both macOS and Linux)
sed -e "s@version:.*@version: $ENUM@" "$VALUES_FILE" > "${VALUES_FILE}.tmp"
mv "${VALUES_FILE}.tmp" "$VALUES_FILE"

echo "${SUCCESS}Successfully updated $VALUES_FILE${NORMAL}"
