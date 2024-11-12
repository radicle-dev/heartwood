Release Process
===============
In this document, we describe the release process for the Radicle binaries. It
is expected to be a living document as we refine our build and release process.

Pre-Release Process
-------------------
Before cutting a proper release, we first aim to cut a pre-release so that we
can test the binaries on a smaller scale, usually internally. To do this, we
follow the following steps, outlined in each subsection.

### Tag Version
The first action required is to create a release tag. All tags that start with a
`v` are considered release tags, e.g. `v1.0.0`, `v1.1.0`, `v1.1.0-pre`, etc.
Before creating the tag, we must decide which commit we are choosing for the
release. In general, this will be the latest commit of the `master` branch. We
checkout this commit:

```
git checkout <commit>
```

The tag name that is being chosen for the pre-release is the next semantic
version, followed by `-pre`. If it is a follow-up pre-release for any fixes, we
append a `.` and digit, e.g. `v1.1.0-pre.2`.

We provide a script for performing the tagging related options, `build/tag`.
The input to this script does not require the `v` prefix. For example, if we
want to cut a release for `v1.1.0-pre`, we would call the script like the
following:

```
build/tag 1.1.0-pre
```

Note that `git config user.signingKey` must match the key you are using as your
Radicle signing key.

The script will ask you to confirm the creation of the tag, respond with `y`
if it all looks good.

### Run Build
The next thing we do is to build the binaries based on the latest tag. We
provide a `build/build` script that performs the build through a Docker
container. The following requirements are needed for running the build script:

* `rad`
* `podman`
* `sha256sum`

Running `build/build` will find the latest tag and perform the build, this will
take a few minutes, so grab a coffee ☕.

---

**Note**: the script currently outputs warnings about the `strip` command for
MacOS builds. These are ok, and can be ignored.

---

### Verify Artifacts
All artifacts constructed from the `build/build` script will be placed under
`build/artifacts`. Any existing, old artifacts can be removed.

We can then verify the artifacts are present via the `build/checksums` script,
which prints the checksum values of all the binaries that were built, noting
that there is a binary for different architectures.

We also check that `build/artifacts/radicle.json` file to see that the metadata
matches what we expected. For example, the output may look something like:

```json
{"name":"rad","version":"1.1.0-pre","commit":"0c9a7419","timestamp":"1729696767"}
```

Making careful note of the `version` and `commit`.

### Upload Artifacts
The next step is to upload the artifacts to our servers, allowing others to
install the binaries, as well as launching the new binaries on our team seed
node.

This is achieved through the `build/upload` script, which requires SSH access to
`files.radicle.xyz`.

Once the files are released we can install the binaries via:

```
curl -O -L https://files.radicle.xyz/releases/latest/radicle-$TARGET.tar.xz
```

where `$TARGET` is the relevant architecture and version.

### Release on Team Node
To help with testing the pre-release internally, we upgrade our team node,
`seed.radicle.xyz`, which is restricted to only replicate from our team's Node
IDs.

If you have SSH access to the server, we start off using:

```
ssh seed@seed.radicle.xyz
```

Once we have access to the seed, we update the server's binaries using the
`update.sh` – passing it the version number that it is being updated to, for
example if we updating `1.1.0-pre`:

```
update.sh 1.1.0-pre
```

This restarts the Radicle services on the machine.

<!-- TODO: verify the version by being able to run `rad node version` which does -->
<!-- not exist yet -->

### Post Changelog

Once all these steps are completed, we can generate the changelog, by first
checking out the relevant tag, and running `scripts/changelog`. This will output
something like the following:

~~~
# 👾 Radicle v1.1.0-pre

Radicle v1.1.0-pre (f7d8f1b8) is released.

## Installation

```
curl -sSf https://radicle.xyz/install | sh -s -- --no-modify-path --version=1.1.0-pre
```

## Notes

* This update is recommended for everyone. No manual intervention is required.

## Changelog

* `f7d8f1b8` **radicle: Distinguish seeding policy types** *<cloudhead@radicle.xyz>*
* `bbb292c8` **node: Don't panic on connection logic error** *<cloudhead@radicle.xyz>*

## Checksums

```
7f0d99609915d28b4333185e42ff14442b9c5d2ec463d3f5f327fe8bb56145ac  radicle-v1.1.0-pre-x86_64-unknown-linux-musl.tar.xz
7d197c8f0a8ab5837610913a1372b9f66214dbd402fed8def65aa6983f01545e  radicle-v1.1.0-pre-x86_64-apple-darwin.tar.xz
312700ca44f0fa1234687819d29426e83509f0663051779a8908fb0c5e3708e4  radicle-v1.1.0-pre-aarch64-apple-darwin.tar.xz
059eaf77539821a5a9e3e82df9fc5f076aca3ca269c80274251c388af6508be3  radicle-v1.1.0-pre-aarch64-unknown-linux-musl.tar.xz
```
~~~

Once we have the output from `scripts/changelog`, we can post to the internal or
pre-release topic in Zulip – naming the topic after the release version name.
Remember to `@all` so that everyone is notified. We also make note that this is
a pre-release for our team to test and make note of any issues in the
`#dogfoood` stream.

In the `Notes` section we make note of any major or breaking changes that were
made in this release.

Here we can define a grace period of how long we wait for the release to be
running until we decide to cut the final release, given that there are no issues
with the pre-release. This grace period can depend on the size and complexity of
the changes.

Release Process
---------------

<!-- TODO: once we run through the whole process, fill this in -->
<!-- TODO: build/release will create the symlink to the latest version -->
<!-- TODO: post to blog, zulip, and social media – should always pin version -->
