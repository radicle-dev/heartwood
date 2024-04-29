# Versioning

This document describes the versioning scheme used by the Radicle Stack, ie.
the package that includes the Radicle CLI (`rad`), Radicle Node
(`radicle-node`) and other binaries. It is not relevant to the Rust *crate*
versions which follow [Semantic Versioning][semver].

[semver]: https://semver.org/

## Format

Versioning of the Radicle Stack is based on Git tags. During the build phase,
we search for the most recent tag that starts with a `v` character, eg.
`v1.0.0`, and use that as the basis for computing the version.

If we're building code that is pointed to by that tag directly, that code will
inherit that version number, with the `v` character stripped. For example:

    1.0.0

If on the other hand, the commit we are building has no version tag pointing to
it, the output of `git describe` is used as-is. This indicates a development
version that is not released and not meant to be packaged or distributed. For
example:

    1.0.0-6-ga3ffe51d

Tags used for versioning are always annotated and signed, and follow the format:

    "v" <major> "." <minor> "." <patch>

When the tag is parsed, we strip the `v` prefix, which results in a version
matching:

    <major> "." <minor> "." <patch>

For pre-releases or release candidates, we add the `-rc` suffix, plus a number.
For example:

    1.0.0-rc.2

These releases are meant to be packaged.

## Semantics

The Radicle version numbers do not follow strict rules, but these are the
guidelines we use:

1. Increment the `<major>` number when significant changes and/or improvements
   are made to the Radicle Stack. This should happen rarely.
2. Increment the `<minor>` number when new features are added or existing
   features are improved in a noticeable way.
3. Increment the `<patch>` number when bugs are fixed, docs are updated or
   features are tweaked.

Unless clearly stated in the documentation, none of the user-facing commands
should be considered stable APIs, and may therefore break in `<minor>` or
`<major>` version updates. If command output is considered stable, and an
effort is made to maintain that stability, this will be stated in the command's
documentation.
