#!/bin/bash
#
# Import a GitHub issue into Radicle.
#
set -e

if ! command -v curl > /dev/null; then
  echo "Error: curl is not installed"
  exit 1
fi

if ! command -v jq > /dev/null; then
  echo "Error: jq is not installed"
  exit 1
fi

if ! command -v rad > /dev/null; then
  echo "Error: rad is not installed"
  exit 1
fi

# Check if the correct number of arguments is provided
if [ "$#" -ne 3 ]; then
  echo "Usage: $0 <org> <repo> <issue>"
  exit 1
fi

owner="$1"
repo="$2"
issue="$3"

url="https://api.github.com/repos/${owner}/${repo}/issues/${issue}"

# Fetch the issue data using the GitHub API
response="$(curl -s "$url")"

# Extract the title and body from the JSON response
title="$(echo "$response" | jq -r '.title')"
body="$(echo "$response" | jq -r '.body')"
labels="$(echo "$response" | jq -r '.labels | .[].name')"

tags=()
for label in $labels; do
  tags+=("--tag" "$label")
done

rad issue open --title "$title" "${tags[@]}" --description "$body" --no-announce
