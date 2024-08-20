It's common that in projects where there are already multiple
delegates that one of those delegates meets someone that they want to
bring into the project. So let's see how Alice and Bob end up inviting
Eve to the project.

First, we'll start off with Alice adding Bob. It's necessary for Bob
to have a fork of the project and Alice must be aware of the fork:

``` ~bob
$ rad clone rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 --scope followed
✓ Seeding policy updated for rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 with scope 'followed'
✓ Fetching rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 from z6MkuPZ…xEuaPUp@[..]..
✓ Fetching rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 from z6MkvVv…Z1Ct4tD@[..]..
✓ Creating checkout in ./heartwood..
✓ Remote alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi added
✓ Remote-tracking branch alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/master created for z6MknSL…StBU8Vi
✓ Repository successfully cloned under [..]/heartwood/
╭────────────────────────────────────╮
│ heartwood                          │
│ Radicle Heartwood Protocol & Stack │
│ 0 issues · 0 patches               │
╰────────────────────────────────────╯
Run `cd ./heartwood` to go to the repository directory.
$ cd ./heartwood
$ git push rad master
```

If Alice wants to ensure that both her and Bob need to agree on merges
to the default branch, she must set the `threshold` to `2` when adding
Bob as a delegate:

``` ~alice (fails)
$ rad id update --repo rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 --title "Add Bob" --description "" --threshold 2 --delegate did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --no-confirm -q
✗ Error: failed to verify delegates for rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2
✗ Error: a threshold of 2 delegates cannot be met, found 1 delegate(s) and the following delegates are missing [did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk]
✗ Hint: run `rad follow did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk` to follow this missing peer
✗ Hint: run `rad sync -f` to attempt to fetch the newly followed peers
✗ Error: fatal: refusing to update identity document
```

We can see that `a threshold of 2 delegates cannot be met` when Alice
attempts to do this. This is because she requires Bob's default branch
to ensure that the threshold can be met and the canonical version of
the default branch (`refs/heads/<default branch>` at the top-level of
the storage) can be updated.

So, instead Alice needs to first follow Bob and fetch his references:

``` ~alice
$ rad follow did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --alias bob
✓ Follow policy updated for z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk (bob)
$ rad sync
✓ Fetching rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 from z6MkuPZ…xEuaPUp@[..]..
✓ Fetching rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 from z6MkvVv…Z1Ct4tD@[..]..
✓ Fetched repository from 2 seed(s)
✓ Synced with 1 node(s)
$ rad id update --repo rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 --title "Add Bob" --description "" --threshold 2 --delegate did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --no-confirm -q
069e7d58faa9a7473d27f5510d676af33282796f
$ rad sync
✓ Fetching rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 from z6MkuPZ…xEuaPUp@[..]..
✓ Fetching rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 from z6MkvVv…Z1Ct4tD@[..]..
✓ Fetched repository from 2 seed(s)
✓ Synced with 3 node(s)
```

Bob can confirm that he was made a delegate by fetching the update:

``` ~bob
$ rad sync
✓ Fetching rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 from z6MkuPZ…xEuaPUp@[..]..
✓ Fetching rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 from z6MkvVv…Z1Ct4tD@[..]..
✓ Fetched repository from 2 seed(s)
✓ Synced with 1 node(s)
$ rad inspect --delegates
did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi (alice)
did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk (bob)
```

Later, Bob meets Eve at a conference and she wants to take part in the
project. For Bob to propose Eve, similar steps need to happen as
between Alice and Bob. Eve first needs to setup a fork:

``` ~eve
$ rad clone rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 --scope followed
✓ Seeding policy updated for rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 with scope 'followed'
✓ Fetching rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 from z6MkuPZ…xEuaPUp@[..]..
✓ Fetching rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 from z6MkvVv…Z1Ct4tD@[..]..
✓ Creating checkout in ./heartwood..
✓ Remote alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi added
✓ Remote-tracking branch alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/master created for z6MknSL…StBU8Vi
✓ Remote bob@z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk added
✓ Remote-tracking branch bob@z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk/master created for z6Mkt67…v4N1tRk
✓ Repository successfully cloned under [..]/heartwood/
╭────────────────────────────────────╮
│ heartwood                          │
│ Radicle Heartwood Protocol & Stack │
│ 0 issues · 0 patches               │
╰────────────────────────────────────╯
Run `cd ./heartwood` to go to the repository directory.
$ cd ./heartwood
$ git push rad master
```

Bob then adds Eve as a delegate:

``` ~bob
$ rad id update --repo rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 --title "Add Eve" --description "" --delegate did:key:z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z --no-confirm -q
3cd3c7f9900de0fcb19705856a7cc339a38fb0b3
$ rad sync
✓ Fetching rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 from z6MkvVv…Z1Ct4tD@[..]..
✓ Fetching rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 from z6MkuPZ…xEuaPUp@[..]..
✓ Fetched repository from 2 seed(s)
✓ Synced with 3 node(s)
```

Notice how there was no need to follow Eve right away in this case?
This is because Bob can meet the threshold of 2 without Eve, he
has Alice and his default reference.

Since there are two delegates when Bob adds Eve, Alice needs to accept
the change to meet a quorum of votes (`votes >= (delegates / 2) + 1`):

``` ~alice
$ rad sync
✓ Fetching rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 from z6MkvVv…Z1Ct4tD@[..]..
✓ Fetching rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 from z6MkuPZ…xEuaPUp@[..]..
✓ Fetched repository from 2 seed(s)
✓ Nothing to announce, already in sync with 3 node(s) (see `rad sync status`)
$ rad id list
╭────────────────────────────────────────────────────────────────────────────────╮
│ ●   ID        Title              Author                     Status     Created │
├────────────────────────────────────────────────────────────────────────────────┤
│ ●   3cd3c7f   Add Eve            bob      z6Mkt67…v4N1tRk   active     now     │
│ ●   069e7d5   Add Bob            alice    (you)             accepted   now     │
│ ●   0656c21   Initial revision   alice    (you)             accepted   now     │
╰────────────────────────────────────────────────────────────────────────────────╯
$ rad inspect rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 --sigrefs
z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi 6acd3b370839318d96dbfff43948bab2bcdd3681
z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk c40018821dc1b41cad75e91e0c9d00827e815324
$ rad id accept 3cd3c7f
✓ Revision 3cd3c7f9900de0fcb19705856a7cc339a38fb0b3 accepted
╭────────────────────────────────────────────────────────────────────────╮
│ Title    Add Eve                                                       │
│ Revision 3cd3c7f9900de0fcb19705856a7cc339a38fb0b3                      │
│ Blob     74581605d1f75396c331487a10ca61c4815ed685                      │
│ Author   did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk      │
│ State    accepted                                                      │
│ Quorum   yes                                                           │
├────────────────────────────────────────────────────────────────────────┤
│ ✓ did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi alice (you) │
│ ✓ did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk bob         │
╰────────────────────────────────────────────────────────────────────────╯
```

At this point, when Alice runs `rad sync`, she will fetch Eve's fork
since she has become a delegate:

``` ~alice
$ rad sync --timeout 3
✓ Fetching rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 from z6MkvVv…Z1Ct4tD@[..]..
✓ Fetching rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 from z6MkuPZ…xEuaPUp@[..]..
✓ Fetched repository from 2 seed(s)
✓ Synced with 3 node(s)
$ rad inspect rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 --sigrefs
z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi 1f716870f890be0c13fdd0af9f527af849fec792
z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk c40018821dc1b41cad75e91e0c9d00827e815324
z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z 95cd447c57de8d232c6154f5dba0451aa593520e
```

Since the network is eventually consistent, if Eve decides to `sync`
(this could also happen through a transient announcement), then we can
see that both seeds are `synced`:

``` ~eve
$ rad sync
✓ Fetching rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 from z6MkvVv…Z1Ct4tD@[..]..
✓ Fetching rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 from z6MkuPZ…xEuaPUp@[..]..
✓ Fetched repository from 2 seed(s)
✓ Synced with 3 node(s)
$ rad sync status
╭─────────────────────────────────────────────────────────────────────────────────────────────────╮
│ ●   Node                            Address                        Status   Tip       Timestamp │
├─────────────────────────────────────────────────────────────────────────────────────────────────┤
│ ●   eve           (you)                                                     95cd447   now       │
│ ●   bob           z6Mkt67…v4N1tRk                                  synced   95cd447   now       │
│ ●   distrustful   z6MkvVv…Z1Ct4tD   distrustful.radicle.xyz:8776   synced   95cd447   now       │
│ ●   seed          z6MkuPZ…xEuaPUp   seed.radicle.xyz:8776          synced   95cd447   now       │
╰─────────────────────────────────────────────────────────────────────────────────────────────────╯
```
