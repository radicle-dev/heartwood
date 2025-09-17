# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## Release Highlights

## Deprecations

- The option `rad self --nid` was deprecated in favor of `rad status --only nid`
- `rad diff` was deprecated in favor of using `git diff`
- `rad patch review --patch` and `rad patch review --delete` are made obsolete.
  This functionality never worked as intended, and may be removed before the
  next major release.

## New Features

- `rad clone` now supports the flag `--bare` which works analoguously to
  `git clone --bare`.
- `rad patch show` now has improved output. It does not distinguish between the
  original author's updates and other updated, each update is marked as
  `Revision`, and the general output is cleaned up. It also shows `Base` by
  default without the `--verbose` flag.
- `rad init --setup-signing` now works on bare repositories.
- `git-remote-rad` now correctly reports the default branch to Git by listing
  the symbolic reference `HEAD`.
- `rad status` learned a new option `--only nid` for printing the Node ID.

## Fixed Bugs

- `rad init --setup-signing` now works in combination with `--existing`.

## 1.4.0

## Release Highlights

## Deprecations

## New Features

- `rad cob log` now supports the arguments `--from` and `--to` which can be used
  to range over particular operations on a COB.

## Fixed Bugs

## 1.3.0 - 2025-08-12

## Release Highlights

### Canonical References

Introduce canonical reference rules via a payload entry in the identity
document. The payload is identified by `xyz.radicle.crefs`, and the payload
currently contains one key `rules`, which is followed by the set of rules. For
each rule, there is a reference pattern string to identify the rule, which in
turn is composed of the `allow` and `threshold` values. The canonical reference
rules are now used to check for canonical updates. The rule for the
`defaultBranch` of an `xyz.radicle.project` is synthesized from the identity
document fields: `threshold` and `delegates`. This means that a rule for that
reference is not allowed within the rule set. This checked when performing a
`rad id update`.

### Introducing `radicle-protocol`

This set of changes is mostly cosmetic for the time being. A new crate,
`radicle-protocol`, was introduced to provide a home for a sans I/O
implementation of the Radicle protocol. The crate currently defines the inner
workings of the protocol, and `radicle-node` depends on this.

Note here that we switched to use the `bytes` crate, and we witnessed a panic
from this crate while using a pre-release. It has not showed up again, but we
introduced the use of backtraces to help identify the issue further. So, please
report a backtrace if the `radicle-node` stops due to this issue.

### Path to Windows

We made an effort to start paving some of the way to being able to use Radicle
on Windows. The first step was taken for this, and you can now use the `rad` CLI
on a Windows machine – without WSL.

Currently, `radicle-node` is still not compatible with Windows.
However, the sans I/O approach mentioned above will provide a way
forward for implementing a `radicle-node` that works on Windows, and we will
continue to look into other fixes required for getting full Windows support.

### Display Full Node IDs

Node IDs and and node addresses have improved formatting. The CLI will output
shortened forms of NIDs and addresses when the output is transient, and the full
form where it is presented to the user. This will allow you to be able to copy
and paste these identifiers.

## New Features

- Canonical reference rule in the identity payload, identified by
  `xyz.radicle.crefs`.
- The `git-remote-rad` executable can now be called from bare repositories and
  can push any kind of Git revision, greatly improving the experience for users
  of `jj`.
- The pinned repositories now maintain their insertion order.
- Improved error reporting during canonical reference calculations. This will
  provide users with more information on error cases that can occur when
  computing canonical references.
- When running `rad init` the default value for the `defaultBranch` of the
  repository is now by provided the branch you are on or the Git configuration
  option `init.defaultBranch`.

## Fixed Bugs

- Connection attempts will now return an error if they fail. Before the change,
  the connection attempts would timeout.

## 1.2.0 - 2025-06-02

### Release Highlights

#### Improved Performance of Repository Initialization

There has been a huge improvement in initialising larger repositories. This was, unfortunately, due to `libgit2` being a lot slower than `git` when performing file protocol push and fetches.

#### Better `rad sync` Output

There has been a concerted effort to improve the fetching and announcing output when using `rad sync`. This also helped us improve `rad clone` which should not include many error messages, while also succeeding.

### New Features

#### CLI

- Output JSON lines for `rad cob`
- Allow showing multiple COBs at once
- Improvements to help documentation
- The full set of actions for patches are now available via `rad patch`
- Better error context when `ssh-agent` connection fails
- The remote helper will print `git range-diff`s when creating new patch revisions
- `rad seed` and `rad unseed` can now take multiple RIDs
- `rad cob [create | update]` have been added
- `rad config schema` for emitting a JSONSchema of the configuration
- Better syntax highlighting
- `rad cob show` handles broken pipes
- Avoiding obtaining a signer when it is not necessary
- Print node addresses when syncing

#### Library

- Patch revisions can now be labelled and resolve comments
- Issues can be listed by status
- Extend the set of emojis that are supported
- Provide an API to do a reverse lookup from aliases to NIDs
- Use `signals_receipts` crate for improved signal handling
- Integrate more up-to-date Gitoxide crates
- Ensuring an MSRV of 1.81

## 1.1.0 - 2024-12-05

### Release Highlights

#### Database Migration

This release includes a migration of the COB database to version 2. The
migration is run automatically when you start your node. If you'd like to run
it manually, use `rad cob migrate`.

#### CLI

* A new `--edit` flag was added to the `rad id update` command, to make changes
  to an identity document from your editor.
* A new `--storage` flag was added to `rad patch cache` and `rad issue cache`
  that operates on the entire storage, instead of a specific repository.
* When fetching a repository with `--seed` specified on the CLI, we now try to
  connect to the seed it if not already connected.
* A new set of sub-commands were added to `rad config`, for directly modifying
  the local Radicle configuration. See `rad config --help` for details.
* Repositories are now initialized with a new refspec for the `rad` remote, that
  ensures that tags are properly namespaced under their remote.
* A new `--remote <name>` flag was added to `rad patch checkout` and `rad patch
  set` to set the remote for those commands. Defaults to `rad`.
* The `RAD_PASSPHRASE` variable is now correctly treated as no passphrase when
  empty.

#### Git Remote Helper

* The `GIT_DIR` environment variable is no longer required for listing refs via
  the remote helper. This means the commands can be run outside of a working
  copy.
* Fixed a bug where the wrong commit was used in the Patch COB when merging
  multiple patches with a single `git push`, resulting in some merged patches
  showing as unmerged.

#### Collaborative Objects (COBs)

* Fixed compatibility with certain old patches that contained empty reviews.
* Added a new `review.edit` action to the `xyz.radicle.patch` COB, for editing
  reviews.

#### Node

* When fetching a repository, the fetch would fail if the canonical branch could
  not be established. This is no longer the case, allowing the user to handle the problem
  locally.
* When fetching a repository, we no longer fail a fetch from a peer that is
  missing a reference to the default branch.
* Private RIDs that could sometimes leak over the gossip protocol no longer do.
  Note that this only affected the identifiers, not any repository data.

#### Protocol

* A new `rad/root` reference is added to the list of signed references
  (`rad/sigrefs`). This prevents a possible reference grafting attack.
