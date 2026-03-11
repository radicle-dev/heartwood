# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## Release Highlights

## New Features

- The block policy for `NodeId`'s is used for limiting the namespaces fetched
  from other nodes. It is now also extended to block connections to the blocked
  `NodeId`.
- The set of references returned by `references_of` were restricted to `heads`,
  `tags`, `notes`, `rad`, and `cobs`. The restriction is lifted, and the only
  references filtered out are `refs/tmp/heads` – used by `radicle-remote-helper`
  to create temporary patches.
- The `rad id` command will provide a better error message when a non-delegate
  attempts to modify the identity document.
- The `journal_mode` and `synchronous` pragmas can now be configured using the
  configuration file. The default values used are `WAL` and `NORMAL`, which
  generates less I/O operations. On power loss, transactions might be rolled
  back, but SQLite still guarantees consistency in this mode.

## Fixed Bugs

- When preparing commands to execute, the `shlex` crate was used on all platforms.
  The semantics on Windows are different (e.g. '\' is a path separator on Windows
  but marks an escape sequence on Unix-like systems), which lead to issues when
  attempting to execute child processes.
  This is fixed by using `winsplit` on Windows instead.
- On Windows, zombie `git-upload-pack` processes are now prevented by using the
  "Job" API of the operating system to group child processes and their children.
- On Windows, signal handling was not supported. The `radicle-node` executable
  will now respect signal handling on Windows.
- Commands sent to the `Service` would never respond when it encountered errors.
  This would result in timeouts when commands are run from the `rad` CLI.
  The `Service` has now learned to return results when an error occurs which
  will be reported back to the user.
- Parsing addresses involving an IPv6 host failed if they were enclosed in
  square brackets, e.g. in `rad node connect z6Mk...@[::1]:8776`.
  Also, ambiguous addresses would parse, e.g. `::1:8776` would be
  indistinguishable from `[::1]:8776`. Since a port number is always required
  along a host when providing an address, IPv6 addresses now always require
  brackets to avoid confusion.

## Deprecations

- The `rad fork` command was confusing, and mislead users as to what its purpose
  was. Many believed it to create a hard-fork of the repository. Instead, it
  pushed the default branch to the local user's namespace. The command is now
  deprecated, and the user should use `git push` instead.

## Breaking Changes

- The `Connected` state of a peer no longer contains fetching information. This
  information was returned when requesting for `Seeds` on the control socket.
  Callers should no longer expect the `fetching` inside that JSON result.
- The `rad node debug` information for ongoing fetches contained the number of
  subscribers awaiting for results, this was removed.
- The `TypeName` strings defined in `radicle-cob` are restricted to reflect the
  size limits on domain names as specified in
  [RFC-1035](https://www.rfc-editor.org/rfc/rfc1035#section-2.3.4).

## 1.6.1

## Fixed Bugs

### Improve Logging

The introduction of new logs in `radicle-node` caused too many log lines to be
output. All logs were evaluated and adjusted to a more suitable log level.

### Improve `Service::fetch_missing_repositories`

If the check for `storage.contains` failed with an error, the whole process of
fetching missing repositories would fail. Instead, the error is logged and it
continues to gather repositories to fetch.

### Surface Underlying I/O Error for `radicle-fetch`

When an I/O error would occur within the `gix-transport` crate, the underlying
error would become opaque. This makes it hard to debug the issue when it occurs.
Instead, surface the I/O error so that it can be inspected.

## 1.6.0

## Release Highlights

### Migrating `radicle-node` to `mio`

The crates [`netservices`], [`io-reactor`], and [`popol`] were crucially valuable
for implementing `radicle-node`. However, they are not ideal dependencies for
ensuring long-term health of the network I/O layer:

- [`popol`] is only intended to support Unix-like platforms, and support on other
  platforms, like Windows, is desired.
- Even though [`io-reactor`] defines the trait [`reactor::poller::Poll`] to
  potentially support multiple I/O polling mechanisms, there is only one single
  implementation wrapping [`popol`]. Issues for other polling crates are open
  since 2023 without tangible progress: [#10 for `mio`], [#9 for `polling`],
  [#8 for `epoll`]. This suggests that it is not a high priority for the maintainers
  to integrate with other polling abstractions which might offer better
  cross-platform compatibility when compared to `popol`.
- The trait [`reactor::poller::Poll`] can only be implemented for file
  descriptors which also implement [`std::os::fd::raw::AsRawFd`], which is only
  implemented on Unix-like platforms and WASI. It is believed that this is
  leaked from `popol` as the only known implementation of the trait wraps it.
- To benefit from network effects, it would be nice to see others maintaining crates
  that depend on `io-reactor`. However, according to crates.io, the
  [only dependent is `radicle-node`] (via `netservices`). Contrary to that,
  at the time of writing, `mio` has 494 dependents according to
  [crates.io][mio reverse dependencies], and, notably, `tokio`, which has
  30628 dependents on [crates.io][tokio reverse dependencies], is dependent on
  [`mio`]. We therefore think that even if `mio` is obsoleted, e.g. by [`a10`]
  (which is based on [`io_uring`] on Linux and could potentially build on top of
  [I/O rings on Windows]) the people behind a large network of dependent projects
  are expected to come up with new ideas and solutions, that Radicle would then
  benefit from.
- One downside of using `mio` is that it forces the use of [`mio::Token`] to
  identify sources (while a type that is `Eq + Clone` might be enough). Another
  downside is that it forces the use of the types in [`mio::net`] for sockets,
  which need to be converted to/from [`std::net`] if required. These
  distinctions are also [noted by cloudhead]. This is acceptable to the team, in
  order to leverage the benefits of a well-tested and cross-platform network I/O
  layer.

[`netservices`]: https://crates.io/crates/netservices
[`io-reactor`]: https://crates.io/crates/io-reactor
[`popol`]: https://crates.io/crates/popol
[`reactor::poller::Poll`]: https://docs.rs/io-reactor/0.5.2/reactor/poller/trait.Poll.html
[#10 for `mio`]: https://github.com/rust-amplify/io-reactor/issues/10
[#9 for `polling`]: https://github.com/rust-amplify/io-reactor/issues/9
[#8 for `epoll`]: https://github.com/rust-amplify/io-reactor/issues/8
[`std::os::fd::raw::AsRawFd`]: https://doc.rust-lang.org/nightly/std/os/fd/raw/trait.AsRawFd.html
[only dependent is `radicle-node`]: https://crates.io/crates/io-reactor/reverse_dependencies
[mio reverse dependencies]: https://crates.io/crates/mio/reverse_dependencies
[tokio reverse dependencies]: https://crates.io/crates/tokio/reverse_dependencies
[`a10`]: https://crates.io/crates/a10
[`io_uring`]: https://en.wikipedia.org/wiki/Io_uring
[I/O rings on Windows]: https://learn.microsoft.com/en-us/windows/win32/api/ioringapi/
[`mio::Token`]: https://docs.rs/mio/1.0.4/mio/struct.Token.html
[`mio::net`]: https://docs.rs/mio/1.0.4/mio/net/index.html
[`std::net`]: https://doc.rust-lang.org/stable/std/net/index.html
[noted by cloudhead]: https://cloudhead.io/popol/

### Building `radicle-node` on Windows

The efforts to migrate `radicle-node` to use `mio`, alongside changes that fixed
path canonicalization and supporting Windows pipes, have allowed developers to
build `radicle-node` on Windows.

We encourage users to try out Radicle on Windows by building from source. At the
time of writing, there may be undiscovered issues, since this is a nascent time
for `radicle-node` on Windows. Please report any issues you see via `rad issue`
or on our [Zulip](https://radicle.zulipchat.com).

### Rust MSRV Update to 1.85

For those who are developing on top of the `heartwood` crates, it is important
to note that the Minimum Supported Rust Version (MSRV) is now 1.85.

## New Features

### Argument Parsing via `clap`

`rad` now uses the `clap` crate for parsing its command-line arguments. This
brings a brand new look to the help output for the `rad` CLI, and ensures that
we do not miss documenting options when they are added. Note that this does
affect error reporting, as they are now reported by `clap` when parsing fails.

#### Shell Completions

With the introduction of `clap`, this helped with the introduction of a command
`rad completion` to emit shell completions for static information.

### systemd Credentials for `radicle-node`

`radicle-node` now supports systemd Credentials (refer to
<https://systemd.io/CREDENTIALS> for more information) to load:
    1. The secret key, in addition to the commandline argument `--secret`
       (higher priority than the credential) and the configuration file (lower
       priority than the credential). The identifier of the credential is
       "xyz.radicle.node.secret".
    2. The optional passphrase for the secret key, in addition to the
       environment variable `RAD_PASSPHRASE` (lower priority than the
       credential). The identifier of the credential is
       "xyz.radicle.node.passphrase".

## Fixed Bugs

### Fix Bootstrapping

The IP (both IPv4 and IPv6) and the Tor onion addresses were specified for the
bootstrap nodes. When a new user came to using Radicle, there was a chance that
their setup did not support IPv6 or Tor, resulting in a failure to connect to
one of those addresses. The node does not know how to try a follow-up address,
for the moment, so we have decided to skip Tor addresses when it is not
configured, and removed the IP addresses in favor of the DNS names.

## 1.5.0

## Release Highlights

### Better Support for Bare Repositories

[gitrepostiory-layout]: https://git-scm.com/docs/gitrepository-layout/2.49.0

Some improvements to supporting bare repositories have been made for `rad` and
`git-remote-rad`. For `rad`, the `rad clone` command has learned a new flag
`--bare`, which clones the repository into a bare repository, as opposed to
having a working tree (see [gitrepository-layout]).

`git-remote-rad` (our Git remote helper), also learned to better handle bare
repositories, when using `git push` and `git fetch` with a `rad://` remote.

For `jj` users, this begins to unlock being able to use `jj` without co-location
of the Git repository. Further improvements to interoperability with `jj` are
in progress and will be released in future versions.

### Introducing the `patch.branch` Option

Continuing on the theme of making `jj` users happy, `git-remote-rad` can now
handle the option `-o patch.branch[=<name>]`. When the option is passed without
a name, i.e. `-o patch.branch`, an upstream branch will be created which is
named after the patch being created – `patches/<PATCH ID>`. Alternatively, the
`<name>` value is used if supplied.

This allows you to specify if you want a tracking branch (or bookmark in `jj`)
for the patch. This means that you can avoid using `rad patch checkout`.

### Improved `rad patch show`

The `rad patch show` command has received some love by improving its output. The
`Base` of the patch is now always output, where before it was behind the
`--verbose` flag.

The previous output would differentiate "updates", where the original author
creates a new revision, and "revisions", where another author creates a
revision. This could be confusing since updates are also revisions. Instead, the
output shows a timeline of the root of the patch and each new revision, without
any differentiation. The revision identifiers, head commit of the revision, and
author are still printed as per usual.

### Structured Logging

The `radicle-node` has learned to output structure logging using the new
`--log-logger structured` and `--log-format json` option pairs. If they are not
specified, then the logging will remain the same as per usual.

### Deprecations in `rad`

It is important to note that we are now emitting deprecation and obsoletion
warnings for several `rad` commands and options.

For `rad diff`, the whole command is deprecated, and `git diff` should be used
instead. It is better to use the tools that already exist in this case.

The option `rad self --nid` was deprecated in favor of `rad node status --only nid`.
The reason for this is that we will be making efforts to separate the cryptographic
identity of user and node.
For this case, the node will – in a future version – read the location of the
secret key to use from configuration or arguments at runtime. This means that a
running node is required to report the correct Node ID – and the command cannot
rely on the default location, which is shared with the user.

The options `rad patch review [--patch | --delete]` are marked as obsolete,
since their functionality never worked as intended. Reviews are something that
requires more research and time to implement. These commands will likely be
removed before a next major release, since their lack of functionality is
confusing.

## Deprecations

- The option `rad self --nid` was deprecated in favor of `rad node status --only nid`
- `rad diff` was deprecated in favor of using `git diff`
- `rad patch review --patch` and `rad patch review --delete` are made obsolete.
  This functionality never worked as intended, and may be removed before the
  next major release.
- The option `radicle-node --log` was deprecated in favor of
  `radicle-node --log-level` to be in line with `--log-logger` and `--log-format`.

## New Features

- `rad clone` now supports the flag `--bare` which works analogously to
  `git clone --bare`.
- `rad patch show` now has improved output. It does not distinguish between the
  original author's updates and other updated, each update is marked as
  `Revision`, and the general output is cleaned up. It also shows `Base` by
  default without the `--verbose` flag.
- `rad init --setup-signing` now works on bare repositories.
- `git-remote-rad` now correctly reports the default branch to Git by listing
  the symbolic reference `HEAD`.
- `rad status` learned a new option `--only nid` for printing the Node ID.
- The remote helper has learned a new server option `patch.branch[=<name>]`.
  This will create an upstream branch when creating the branch. This upstream
  can then be used for updating the patch, post creation.
- `radicle-node` has learned `--log-logger structured` and `--log-format json`
  options. The node will output its logs in a structured, JSON format when
  specified.

## Fixed Bugs

- The `rad` CLI now uses [indicatif](https://crates.io/crates/indicatif) for
  emitting progress spinners. This fixes an issue when the terminal size was
  too small for the spinner line. It also fixes when there is a user interrupt,
  the cursor would disappear.
- The remote helper will no longer attempt to verify Git hooks twice, when
  performing a `git push`.
- The default Git remote options, when using `rad remote`, now set `pruneTags`
  to prevent canonical tags from being pruned from the working copy of the
  repository's `refs/tags`.
- `rad init --setup-signing` now works in combination with `--existing`.

## 1.4.0

## Release Highlights

### systemd service hardening

Running `radicle-node` as systemd service using our service files, will now run the service with some hardening options enabled.
This work includes some trivial sandboxing options in the provided service files and lead users to `systemd-analyze security`.

While being a trivial change and far from a secure service it is an improvement and may push downstream packagers and / or users to add even a bit of sandboxing.

### Path to Windows

We continued working on Windows support for Radicle and made some progress on the node implementation.
As `std::os::unix` is obviously not available on Windows, we resorted to using the `winpipe` crate.
This crate implements a very similar API to `std::os::unix` but for named pipes.
The node has learned how to use named pipes when for the control socket when on Windows architecture.

### Bootstrapping Improvements

When you start a fresh node, it'll need to have at least one seed that it can bootstrap from.
We do this by using `iris.radicle.xyz` and `rosa.radicle.xyz` as bootstrap nodes.
With this release, a node can now connect to them when DNS is not available or a connection via Tor is desired.

### Improvements to `rad cob log`

The rad cob log command learned two new options, `--from` and `--to`.
These take a commit SHA that correspond to a COB operation,
and allows you to limit the log to start from or end the log at those operations, respectively.

### Improvements to rad sync

We now use a more suitable symbol in rad sync status for the status:

✗ Hint:
   ? … Status:
       ✓ … in sync          ✗ … out of sync
       ! … not announced    • … unknown

This aligns closer with the rad node status output. As well as this,
the Tip column was renamed to SigRefs, since the term Tip was too ambiguous.

The internal logic of rad sync --announce was improved by writing more tests and finding edge cases to fix.
Included in these improvements is changing the target behavior.
Before, the announcements would attempt to reach the preferred seeds target and the replication factor.
Now, it tries to reach the preferred seeds and falls back to the replication factor.

### Human Oriented Panics

The `rad` CLI now prints a more human-friendly message when it encounters a panic.

### Notable Crate Changes

- Introduce a new module that provides an API for iterating over a COB's operations, given a range of commits
- Remove `anyhow` from `radicle-term` and `radicle-node`
- BREAKING: Removed `radicle::node::DEFAULT_SOCKET_NAME`, use `radicle::profile::Home::socket` instead
- BREAKING: Add a node event for canonical reference updates

## Fixed Bugs

- Fix panic when reading from SQLite database fails

## 1.3.1 - 2025-09-04

## Fixed Bugs

### Fixed Panics

Two instances of panics were fixed in this release.

The first, and most important, was a panic around serializing wire messages.
There is a strict size limit on the protocol messages that we control. However,
this size limit is not intended to be imposed on Git streams, for example during
fetching from other nodes. We incorrectly placed a check for this size limit in
the `serialize` function, which meant it would panic for some Git fetches. This
was fixed by moving the check elsewhere, while also improving the code so we do
not make that mistake again.

The second involved using the `read` method from the `sqlite` crate. This method
calls `try_read` and `unwrap`s the `Result`, which would cause a panic. We have
replaced the calls to `read` with `try_read` to more gracefully handle the
error.


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
