``` ~alice
$ cd heartwood
$ rad inbox
Your inbox is empty.
```

``` ~bob
$ cd heartwood
$ rad issue open --title "No license file" --description "..." -q
✓ Synced with 1 node(s)
$ git commit -m "Change copyright" --allow-empty -q
$ git push rad HEAD:bob/copy
$ cd ..
$ cd radicle-git
$ git commit -m "Change copyright" --allow-empty -q
$ git push rad -o patch.message="Copyright fixes" HEAD:refs/patches
```

``` ~alice
$ rad inbox --sort-by id
╭──────────────────────────────────────────────────────────────────────╮
│ heartwood                                                            │
├──────────────────────────────────────────────────────────────────────┤
│ 001   ●   [ ... ]    No license file    issue    open      bob   now │
│ 002   ●   bob/copy   Change copyright   branch   created   bob   now │
╰──────────────────────────────────────────────────────────────────────╯
```

``` ~alice
$ rad inbox --all --sort-by id
╭────────────────────────────────────────────────────────────────╮
│ radicle-git                                                    │
├────────────────────────────────────────────────────────────────┤
│ 003   ●   [ ... ]   Copyright fixes   patch   open   bob   now │
╰────────────────────────────────────────────────────────────────╯
╭──────────────────────────────────────────────────────────────────────╮
│ heartwood                                                            │
├──────────────────────────────────────────────────────────────────────┤
│ 001   ●   [ ... ]    No license file    issue    open      bob   now │
│ 002   ●   bob/copy   Change copyright   branch   created   bob   now │
╰──────────────────────────────────────────────────────────────────────╯
```

``` ~alice
$ rad inbox show 2
commit 141c9073066e3910f1dfe356904a0120542e1cc9
Author: radicle <radicle@localhost>
Date:   Thu Dec 15 17:28:04 2022 +0000

    Change copyright

commit f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354
Author: anonymous <anonymous@radicle.xyz>
Date:   Mon Jan 1 14:39:16 2018 +0000

    Second commit

commit 08c788dd1be6315de09e3fe09b5b1b7a2b8711d9
Author: anonymous <anonymous@radicle.xyz>
Date:   Mon Jan 1 14:39:16 2018 +0000

    Initial commit
```

``` ~alice
$ rad inbox list --sort-by id
╭──────────────────────────────────────────────────────────────────────╮
│ heartwood                                                            │
├──────────────────────────────────────────────────────────────────────┤
│ 001   ●   [ ... ]    No license file    issue    open      bob   now │
│ 002       bob/copy   Change copyright   branch   created   bob   now │
╰──────────────────────────────────────────────────────────────────────╯
```

``` ~alice
$ rad inbox show 1
╭──────────────────────────────────────────────────╮
│ Title   No license file                          │
│ Issue   [ ...                                  ] │
│ Author  bob z6Mkt67…v4N1tRk                      │
│ Status  open                                     │
│                                                  │
│ ...                                              │
╰──────────────────────────────────────────────────╯
```

``` ~alice
$ rad inbox clear 1 2
✓ Cleared 2 item(s) from your inbox
$ rad inbox
Your inbox is empty.
$ rad inbox --all
╭────────────────────────────────────────────────────────────────╮
│ radicle-git                                                    │
├────────────────────────────────────────────────────────────────┤
│ 003   ●   [ ... ]   Copyright fixes   patch   open   bob   now │
╰────────────────────────────────────────────────────────────────╯
```

``` ~alice
$ rad inbox clear --all
✓ Cleared 1 item(s) from your inbox
```

``` ~alice
$ rad inbox clear --all
Your inbox is empty.
```

Now let's do an identity update.

``` ~alice
$ rad id update --title "Modify description" --description "Use website" --payload xyz.radicle.project description '"https://radicle.xyz"' -q
[..]
$ rad sync -a
✓ Synced with 1 node(s)
```

``` ~bob
$ rad inbox --all
╭──────────────────────────────────────────────────────────────────────╮
│ heartwood                                                            │
├──────────────────────────────────────────────────────────────────────┤
│ 001   ●   [ ... ]   Modify description   id   accepted   alice   now │
╰──────────────────────────────────────────────────────────────────────╯
$ rad inbox show 1
{
  "payload": {
    "xyz.radicle.project": {
      "defaultBranch": "master",
      "description": "https://radicle.xyz",
      "name": "heartwood"
    }
  },
  "delegates": [
    "did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi"
  ],
  "threshold": 1
}
```
