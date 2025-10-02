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
`releases/` are considered release tags, e.g. `releases/1.0.0`, `releases/1.1.0`,
`releases/1.1.0-rc`, etc.
Before creating the tag, we must decide which commit we are choosing for the
release. In general, this will be the latest commit of the `master` branch. We
checkout this commit:

```
git checkout <commit>
```

The tag name that is being chosen for the release candidate is the next semantic
version, followed by `-rc.1`. If it is a follow-up release candidate for any
fixes, we increase digit, e.g. `releases/1.1.0-rc.2`, `releases/1.1.0-rc.3`,
etc.

Note that, for the next part, `git config user.signingKey` must match the key
you are using as your Radicle signing key, and it must be using the `ssh`
format. In your working copy of `heartwood` you can set this up with the
following commands:

```
git config set gpg.format ssh
```

```
git config set user.signingKey "key::$(rad self --ssh-key)""
```

We provide a script for performing the tagging related options, `build/tag`.
The input to this script does not require the `releases/` prefix. For example,
if we want to cut a release for `releases/1.3.0-rc.3`, we would call the
script like the following:

```
build/tag 1.3.0-rc.3
```

The script will ask you to confirm the creation of the tag, showing you the
commit that you're tagging, respond with `y` if it all looks good.

### Run Build
The next thing we do is to build the binaries based on the latest tag. We
provide a `build/build` script that performs the build through a Docker
container. The following requirements are needed for running the build script:

* `rad`
* `podman`
* `sha256sum`

Running `build/build` will find the latest tag and perform the build, this will
take some time, so grab a coffee ☕.

---

**Note**: the script currently outputs warnings about the `strip` command for
macOS builds. These are ok, and can be ignored.

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
{"name":"rad","version":"1.3.0-rc.3","commit":"3296de8323b5782ff2af9d3a0fe2309a9bf1d3d6","timestamp":"1756131991"}
```

Making careful note of the `version` and `commit`.

### Upload Artifacts
The next step is to upload the artifacts to our servers, allowing others to
install the binaries, as well as launching the new binaries on our team seed
node.

This is achieved through the `build/upload` script, which requires SSH access to
`files.radicle.dev`, for example:

```
SSH_LOGIN=<user> build/upload 1.3.0-rc.3
```

Once the files are released we can install the binaries via:

```
curl -O -L https://files.radicle.dev/releases/latest/radicle-$TARGET.tar.xz
```

where `$TARGET` is the relevant architecture and version.

### Release on Team Node
To help with testing the pre-release internally, we upgrade our team node,
`seed.radicle.dev`, which is restricted to only replicate from our team's Node
IDs.

We do this using NixOS and the [`radicle-nix`][radicle-nix] and
[`radicle-infra`][radicle-infra] repositories.

### Post Changelog

<!-- The examples will obviously need a bit of rework, and probably based on -->
<!-- an upcoming pre-release rather than something historical.  /RL -->

Once all these steps are completed, we can generate the changelog, by first
checking out the relevant tag, and running `scripts/changelog` – you can also
pass a previous version as `--from-version`. This will output something like the
following:

~~~
# 👾 Radicle 1.5.0-rc.2

Radicle 1.5.0-rc.2 (7b00bf2e3) is released.

## Installation

```
curl -sSf https://radicle.dev/install | sh -s -- --no-modify-path --version=1.5.0-rc.2
```

## Notes

* Properly deprecate `rad self --nid` and introduce `rad status --only nid`
* Deprecates `rad diff`
* Obsolete warning for `rad patch review [--patch | --delete]`

## Changelog

This release contains 69 commit(s) by 5 contributor(s).

* `7b00bf2e3` **cli/patch/review: Obsoletion Warning** *<lorenz.leutgeb@radicle.xyz>*
* `8dd17e2a6` **cli/warning: Add `fn obsolete`** *<lorenz.leutgeb@radicle.xyz>*
* `7d1db6a01` **cli/diff: Deprecation Warning** *<lorenz.leutgeb@radicle.xyz>*
* `8558cc223` **cli/self: `--nid` deprecation warning to stderr** *<lorenz.leutgeb@radicle.xyz>*
* `3fb04623a` **cli/warning: Add `fn deprecate`** *<lorenz.leutgeb@radicle.xyz>*
* `2635562c9` **cli/node/status: Add `--only nid`** *<lorenz.leutgeb@radicle.xyz>*
* `8afd55ff6` **build: update release files location** *<fintan.halpenny@gmail.com>*
* `d2e10fdef` **cli/tests/commands: Clean up test `rad_patch`** *<erik@zirkular.io>*
* `19210faab` **protocol/service: Change `Routing table updated..` from info to debug** *<me@sebastinez.dev>*
* `86472fdcc` **remote-helper/fetch: Improve error handling** *<lorenz.leutgeb@radicle.xyz>*
[..]

## Checksums

```
675c9d9731751de9c81f8be5445ac80a5bd6dcc7c5d1718d4d8671b7bdfa69e6  radicle-1.5.0-rc.2-aarch64-unknown-linux-musl.tar.xz
583921069b031789debbd64de86635f0e3e705d742e1e8e619659181b2933c60  radicle-1.5.0-rc.2-aarch64-apple-darwin.tar.xz
fc6ee5d764941aaf21d33547e837f3908fbddba533a5b17675ae04e1ab68a664  radicle-1.5.0-rc.2-x86_64-unknown-linux-musl.tar.xz
166bd82760ac4acf68dc7ba7cfe5f32c490311184def9a387b8e47fd39e28b34  radicle-1.5.0-rc.2-x86_64-apple-darwin.tar.xz
```
~~~

Once we have the output from `scripts/changelog`, we can post to the internal or
release candidate topic in Zulip – naming the topic after the release version name.
Remember to `@all` so that everyone is notified. Issues that are encountered
should be reported in the Zulip topic, so that they can be resolved for the
final release.

In the `Notes` section we make note of any major or breaking changes that were
made in this release.

Here we can define a grace period of how long we wait for the release to be
running until we decide to cut the final release, given that there are no issues
with the pre-release. This grace period can depend on the size and complexity of
the changes.

Release Process
---------------
Once the team feels that the release is ready, the final release can be made.
The `build/tag` step should be repeated for the tag, without the `-rc` suffix.
The `build/build` and `build/upload` steps are repeated.

Finally, `SSH_LOGIN=<user> build/release <version>` is used to create a symlink
from version release to the `latest` release – which is used in our install
script linked to on [Get Started][website].

### Release Branch

At this point, a release branch should be created. This branch will be used for
*patch releases*, e.g. `1.5.1`, `1.5.2`, etc.

The branch must be named `releases/x.y`, similar to the tagged release, where
`x` is the major version, and `y` is the minor version, e.g `releases/1.5`,
`releases/1.6`, etc.

### CHANGELOG

The `heartwood/CHANGELOG.md` must be updated to reflect the latest changes that
were made with regards to the binaries. Many of them should have been included
during the development process, such as new features, breaking changes, or fixed
bugs. It is still worth checking `scripts/changelog` to see if there were any
missed notes.

Once the change log is finalized with a header using the version number, e.g.
`## 1.5.0`, it should be committed to the `releases/x.y` branch and a patch must
be made to port the changes to the `master` branch.

### Announcement

The announcement post is prepared using the [`radicle.xyz`
repository][radicle-xyz], and should appear in the [Updates][updates] section of
the website. The announcement is essentially the same as the
`heartwood/CHANGELOG.md` entry, but should include some preamble about the
effort of the release – have fun with it!

We then announce on [Zulip][zulip], [Mastodon][mastodon], and [Bluesky][bsk].

Patch Releases
--------------

After the `x.y.0` release is made, it may be beneficial, or even necessary, to
release patch releases of the binaries. These patch releases must be compatible
with minor version that was released; otherwise, the commits should not be
included.

These changes may have been made on `master` and back-ported to the
`releases/x.y` branch. Note that is not the job of the maintainer to ensure that
the change applies cleanly to both branches – it is the job of the person
contributing the changes.
Alternatively, it may be the case that changes are made on the `releases/x.y`
branch and forward-ported to `master`. The burden of ensuring changes apply
remains the same as above.

Remember that `heartwood/CHANGELOG.md` must be updated to include the latest
changes in the patch release. These must be forward-ported to the `master`
branch.

[radicle-infra]: https://radicle.network/nodes/seed.radicle.dev/rad:z254T5p17bdFPmzfDojsdjo4HjpoZ
[radicle-nix]: https://github.com/radicle-nix/radicle-nix
[get-started]: https://radicle.dev/#get-started
[radicle-xyz]: https://radicle.network/nodes/seed.radicle.dev/rad:z371PVmDHdjJucejRoRYJcDEvD5pp
[updates]: https://radicle.dev/#updates
[zulip]: https://radicle.zulipchat.com/#narrow/channel/409174-announcements
[mastodon]: https://toot.radicle.dev/@radicle
[bsky]: https://bsky.app/profile/radicle.dev
