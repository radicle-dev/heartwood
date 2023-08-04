#!/bin/sh
# Delete all remote branches that don't have a local copy.

remote="rad"
remote_branches="$(git for-each-ref --format='%(refname:short)' refs/remotes/rad)"

# Check that a branch isn't a remote tracking branch.
is_remote_branch() {
  for remote_branch in $remote_branches; do
    # Remove the "rad/" prefix
    if [ "$1" = "${remote_branch#rad/}" ]; then
      return 0
    fi
  done
  return 1
}

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
    if ! is_remote_branch "$branch"; then
      git push -o "no-sync" $remote --delete "$branch"
      echo "Deleted '$branch'"
    else
      echo "Skipping remote branch '$branch'"
    fi
  fi
done

rad sync --announce
