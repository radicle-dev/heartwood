# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## Release Highlights

## Deprecations

## New Features

## Fixed Bugs

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
