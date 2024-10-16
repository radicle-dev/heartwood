#!/bin/sh

# This is a helper script for finding and replacing the unique hashes in our
# tests, primarily the `radicle-cli` tests.
#
# Whenever we change the format of something that produces a hash, many tests
# can end up failing since the hashes have changes. This script allows you to
# specify a list of replacements in one go to more quickly fix the tests.
#
# Usage:
#
# scripts/find-replace-tests.sh <directory> [<old>:<new>..]
#
# Example:
#
# scripts/find-replace-tests.sh radicle-cli/examples \
#    773b9aab58b11e9fa83d0ed0baca2bea6ff889c9:af6d47e7b9fa48e22cad2b0bb8283e218ae8aed8 \
#    670d02794aa05afd6e0851f4aa848bc87c4712c7:a9dbe8234353319fa7f1d9cfdc844e9d50680eba

# Early exit if any command fails
set -e

# Function to print error messages and exit
function error_exit {
  echo "Error: $1" >&2
  exit 1
}

# Function to check for empty strings and invalid characters
function validate_pair {
  local original="$1"
  local new="$2"

  # Check if either string is empty
  if [[ -z "$original" || -z "$new" ]]; then
    error_exit "Both original and new strings in a pair must be non-empty. Received: '$original' -> '$new'."
  fi

  # Restrict invalid characters (for simplicity, we disallow colons and slashes)
  if [[ "$original" =~ [/:] || "$new" =~ [/:] ]]; then
    error_exit "Invalid characters detected in the strings. Colons and slashes are not allowed."
  fi
}

# Function to escape special characters for sed
function escape_string {
  echo "$1" | sed -e 's/[&/\.*^$[]/\\&/g'
}

# Ensure the directory and at least one pair is provided
if [[ $# -lt 2 ]]; then
  error_exit "Usage: $0 [--dry-run] <directory> '<original>:<new>' ['<original>:<new>' ...]"
fi

# Check for dry-run flag
DRY_RUN=false
if [[ "$1" == "--dry-run" ]]; then
  DRY_RUN=true
  shift # Remove the dry-run argument
fi

# Assign the first argument as the directory
DIRECTORY=$1
shift  # Shift the arguments to leave only the pairs

# Verify that the directory exists
if [[ ! -d $DIRECTORY ]]; then
  error_exit "The specified directory does not exist: $DIRECTORY"
fi

# Loop over each pair and perform validation and find-and-replace
for PAIR in "$@"; do
  # Ensure the pair is in the correct format
  if [[ $PAIR != *:* ]]; then
    error_exit "Invalid pair format: $PAIR. Expected format '<original>:<new>'"
  fi

  # Split the pair into original and new strings
  ORIGINAL=${PAIR%%:*}
  NEW=${PAIR##*:}

  # Validate the pair
  validate_pair "$ORIGINAL" "$NEW"

  # Escape special characters for both original and new strings
  ESCAPED_ORIGINAL=$(escape_string "$ORIGINAL")
  ESCAPED_NEW=$(escape_string "$NEW")

  # Find files that contain the original string
  MATCHING_FILES=$(grep -rl "$ORIGINAL" "$DIRECTORY")

  if [[ -z "$MATCHING_FILES" ]]; then
    echo "No matches found for '$ORIGINAL' in $DIRECTORY"
    continue
  fi

  # Perform the find and replace using sed, only on matching files
  if [ "$DRY_RUN" = true ]; then
    echo "Dry-run: Replacing '$ORIGINAL' with '$NEW' in the following files:"
    echo "$MATCHING_FILES"
  else
    echo "Replacing '$ORIGINAL' with '$NEW' in the following files:"
    echo "$MATCHING_FILES"
    echo "$MATCHING_FILES" | xargs sed -i "s/$ESCAPED_ORIGINAL/$ESCAPED_NEW/g"
  fi

done

if [ "$DRY_RUN" = true ]; then
  echo "Dry-run completed. No files were modified."
else
  echo "Find and replace operations completed successfully."
fi
