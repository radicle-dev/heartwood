Well known COBs, for example issues and patches, can not only be showed via porcelain commands such as
`rad issue show` and `rad patch show`, but also using the plumbing command `rad cob show`.
While humans likely prefer to use `rad issue show` and `rad patch show`, this command makes integration
with other software components easier.

First create an issue.

```
$ rad issue open --title "spice harvester broken" --description "Fremen have attacked, maybe we went too far?" --no-announce
╭──────────────────────────────────────────────────╮
│ Title   spice harvester broken                   │
│ Issue   9de644864342d7a505eb8d58d1ef20e5bb05de2e │
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
│ ●   9de6448   spice harvester broken   alice    (you)                        now    │
╰─────────────────────────────────────────────────────────────────────────────────────╯
```

Let's create a patch, too.

```
$ git checkout -b spice-harvester-broken
$ touch TREATY.md
$ git add TREATY.md
$ git commit -v -m "Start drafting peace treaty"
[spice-harvester-broken 87bbb39] Start drafting peace treaty
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 TREATY.md
$ git push rad -o patch.message="Start drafting peace treaty" -o patch.message="See details." HEAD:refs/patches
```

Patch can be listed.

```
$ rad patch
╭───────────────────────────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title                        Author         Reviews  Head     +   -   Updated  Labels │
├───────────────────────────────────────────────────────────────────────────────────────────────────┤
│ ●  a7ed753  Start drafting peace treaty  alice   (you)  -        87bbb39  +0  -0  now             │
╰───────────────────────────────────────────────────────────────────────────────────────────────────╯
```

Both issue and patch COBs can be listed.

```
$ rad cob list --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type xyz.radicle.issue
9de644864342d7a505eb8d58d1ef20e5bb05de2e
$ rad cob list --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type xyz.radicle.patch
a7ed7534fa15b7a7a7097873287835f3464067c0
```

We can show the issue COB.

```
$ rad cob show --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type xyz.radicle.issue --object 9de644864342d7a505eb8d58d1ef20e5bb05de2e
{"assignees":[],"title":"spice harvester broken","state":{"status":"open"},"labels":[],"thread":{"comments":{"9de644864342d7a505eb8d58d1ef20e5bb05de2e":{"author":"z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi","reactions":[],"resolved":false,"body":"Fremen have attacked, maybe we went too far?","edits":[{"author":"z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi","timestamp":1671125284000,"body":"Fremen have attacked, maybe we went too far?","embeds":[]}]}},"timeline":["9de644864342d7a505eb8d58d1ef20e5bb05de2e"]}}
```

We can show the patch COB too.

```
$ rad cob show --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type xyz.radicle.patch --object a7ed7534fa15b7a7a7097873287835f3464067c0
{"title":"Start drafting peace treaty","author":{"id":"did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi"},"state":{"status":"open"},"target":"delegates","labels":[],"merges":{},"revisions":{"a7ed7534fa15b7a7a7097873287835f3464067c0":{"id":"a7ed7534fa15b7a7a7097873287835f3464067c0","author":{"id":"did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi"},"description":[{"author":"z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi","timestamp":1671125284000,"body":"See details.","embeds":[]}],"base":"4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927","oid":"87bbb394a79a73803c39f34d2849772c533f56ce","discussion":{"comments":{},"timeline":[]},"reviews":{},"timestamp":1671125284000,"resolves":[],"reactions":[]}},"assignees":[],"timeline":["a7ed7534fa15b7a7a7097873287835f3464067c0"],"reviews":{}}
```

Finally let's update the issue and see the output of `rad cob show` also changes.

```
$ rad issue label 9de6448 --add bug --no-announce
$ rad cob show --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type xyz.radicle.issue --object 9de644864342d7a505eb8d58d1ef20e5bb05de2e
{"assignees":[],"title":"spice harvester broken","state":{"status":"open"},"labels":["bug"],"thread":{"comments":{"9de644864342d7a505eb8d58d1ef20e5bb05de2e":{"author":"z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi","reactions":[],"resolved":false,"body":"Fremen have attacked, maybe we went too far?","edits":[{"author":"z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi","timestamp":1671125284000,"body":"Fremen have attacked, maybe we went too far?","embeds":[]}]}},"timeline":["9de644864342d7a505eb8d58d1ef20e5bb05de2e"]}}
```
