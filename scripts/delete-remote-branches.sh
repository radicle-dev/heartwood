#!/bin/sh
# Delete all remote branches that don't have a local copy.

remote="rad"

# Iterate over all remote branches.
for branch in $(git branch -r --format "%(refname:short)"); do
  # Extract the branch name without the "$remote/" prefix.
  branch=${branch#"$remote/"}
  # Never delete the master branch.
  if [ "$branch" == "master" ]; then
    continue
  fi

  # Check if the branch doesn't exist locally.
  if ! git rev-parse --quiet --verify "$branch" >/dev/null; then
    git push -o "no-sync" $remote --delete "$branch"
    echo "Deleted '$branch'"
  fi
done

rad sync --announce
