Well known COBs, for example issues and patches, can not only be showed via porcelain commands such as
`rad issue show` and `rad patch show`, but also using the plumbing command `rad cob show`.
While humans likely prefer to use `rad issue show` and `rad patch show`, this command makes integration
with other software components easier.

First create an issue.

```
$ rad issue open --title "spice harvester broken" --description "Fremen have attacked, maybe we went too far?" --no-announce
╭──────────────────────────────────────────────────╮
│ Title   spice harvester broken                   │
│ Issue   fa09289336f9317e0d2573372f7965cf8861d04e │
│ Author  alice (you)                              │
│ Status  open                                     │
│                                                  │
│ Fremen have attacked, maybe we went too far?     │
╰──────────────────────────────────────────────────╯
```

The issue is now listed under our project.

```
$ rad issue list
╭─────────────────────────────────────────────────────────────────────────────────────╮
│ ●   ID        Title                    Author           Labels   Assignees   Opened │
├─────────────────────────────────────────────────────────────────────────────────────┤
│ ●   fa09289   spice harvester broken   alice    (you)                        now    │
╰─────────────────────────────────────────────────────────────────────────────────────╯
```

Let's create a patch, too.

```
$ git checkout -b spice-harvester-broken
$ touch TREATY.md
$ git add TREATY.md
$ git commit -v -m "Start drafting peace treaty"
[spice-harvester-broken 575ed68] Start drafting peace treaty
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 TREATY.md
$ git push rad -o patch.message="Start drafting peace treaty" -o patch.message="See details." HEAD:refs/patches
```

Patch can be listed.

```
$ rad patch
╭───────────────────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title                        Author         Reviews  Head     +   -   Updated │
├───────────────────────────────────────────────────────────────────────────────────────────┤
│ ●  07f94cd  Start drafting peace treaty  alice   (you)  -        575ed68  +0  -0  now     │
╰───────────────────────────────────────────────────────────────────────────────────────────╯
```

Both issue and patch COBs can be listed.

```
$ rad cob list --repo rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 --type xyz.radicle.issue
fa09289336f9317e0d2573372f7965cf8861d04e
$ rad cob list --repo rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 --type xyz.radicle.patch
07f94cddadbf87eca62a4b175c47b03db3015427
```

We can show the issue COB.

```
$ rad cob show --repo rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 --type xyz.radicle.issue --object fa09289336f9317e0d2573372f7965cf8861d04e
{"assignees":[],"title":"spice harvester broken","state":{"status":"open"},"labels":[],"thread":{"comments":{"fa09289336f9317e0d2573372f7965cf8861d04e":{"author":"z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi","reactions":[],"resolved":false,"body":"Fremen have attacked, maybe we went too far?","edits":[{"author":"z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi","timestamp":1671125284000,"body":"Fremen have attacked, maybe we went too far?","embeds":[]}]}},"timeline":["fa09289336f9317e0d2573372f7965cf8861d04e"]}}
```

We can show the patch COB too.

```
$ rad cob show --repo rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 --type xyz.radicle.patch --object 07f94cddadbf87eca62a4b175c47b03db3015427
{"title":"Start drafting peace treaty","author":{"id":"did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi"},"state":{"status":"open"},"target":"delegates","labels":[],"merges":{},"revisions":{"07f94cddadbf87eca62a4b175c47b03db3015427":{"id":"07f94cddadbf87eca62a4b175c47b03db3015427","author":{"id":"did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi"},"description":[{"author":"z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi","timestamp":1671125284000,"body":"See details.","embeds":[]}],"base":"f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354","oid":"575ed68c716d6aae81ea6b718fd9ac66a8eae532","discussion":{"comments":{},"timeline":[]},"reviews":{},"timestamp":1671125284000,"resolves":[],"reactions":[]}},"assignees":[],"timeline":["07f94cddadbf87eca62a4b175c47b03db3015427"],"reviews":{}}
```

Finally let's update the issue and see the output of `rad cob show` also changes.

```
$ rad issue label fa09289 --add bug --no-announce
$ rad cob show --repo rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 --type xyz.radicle.issue --object fa09289336f9317e0d2573372f7965cf8861d04e
{"assignees":[],"title":"spice harvester broken","state":{"status":"open"},"labels":["bug"],"thread":{"comments":{"fa09289336f9317e0d2573372f7965cf8861d04e":{"author":"z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi","reactions":[],"resolved":false,"body":"Fremen have attacked, maybe we went too far?","edits":[{"author":"z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi","timestamp":1671125284000,"body":"Fremen have attacked, maybe we went too far?","embeds":[]}]}},"timeline":["fa09289336f9317e0d2573372f7965cf8861d04e"]}}
```
