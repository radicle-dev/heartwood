It's common that in projects where there are already multiple
delegates that one of those delegates meets someone that they want to
bring into the project. So let's see how Alice and Bob end up inviting
Eve to the project.

First, we'll start off with Alice adding Bob. It's necessary for Bob
to have a fork of the project and Alice must be aware of the fork:

``` ~bob
$ rad clone rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --scope followed
✓ Seeding policy updated for rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji with scope 'followed'
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6MkuPZ…xEuaPUp..
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6MkvVv…Z1Ct4tD..
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

At this point Alice can follow Bob, fetch his fork, and add him to the delegate
set:

``` ~alice
$ rad follow did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --alias bob
✓ Follow policy updated for z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk (bob)
$ rad sync
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6MkvVv…Z1Ct4tD..
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6MkuPZ…xEuaPUp..
✓ Fetched repository from 2 seed(s)
✓ Nothing to announce, already in sync with network (see `rad sync status`)
$ rad id update --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --title "Add Bob" --description "" --threshold 2 --delegate did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --no-confirm -q
069e7d58faa9a7473d27f5510d676af33282796f
$ rad sync
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6MkvVv…Z1Ct4tD..
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6MkuPZ…xEuaPUp..
✓ Fetched repository from 2 seed(s)
✓ Synced with 2 node(s)
```

Bob can confirm that he was made a delegate by fetching the update:

``` ~bob
$ rad sync
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6MkvVv…Z1Ct4tD..
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6MkuPZ…xEuaPUp..
✓ Fetched repository from 2 seed(s)
✓ Nothing to announce, already in sync with network (see `rad sync status`)
$ rad inspect --delegates
did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi (alice)
did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk (bob)
```

Later, Bob meets Eve at a conference and she wants to take part in the
project. For Bob to propose Eve, similar steps need to happen as
between Alice and Bob. Eve first needs to setup a fork:

``` ~eve
$ rad clone rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --scope followed
✓ Seeding policy updated for rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji with scope 'followed'
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6MkvVv…Z1Ct4tD..
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6MkuPZ…xEuaPUp..
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

Bob then follows Eve and fetches her fork and adds her as a delegate:

``` ~bob
$ rad follow did:key:z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z --alias eve
✓ Follow policy updated for z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z (eve)
$ rad sync
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6MkvVv…Z1Ct4tD..
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6MkuPZ…xEuaPUp..
✓ Fetched repository from 2 seed(s)
✓ Nothing to announce, already in sync with network (see `rad sync status`)
$ rad id update --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --title "Add Eve" --description "" --delegate did:key:z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z --no-confirm -q
3cd3c7f9900de0fcb19705856a7cc339a38fb0b3
$ rad sync
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6MkvVv…Z1Ct4tD..
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6MkuPZ…xEuaPUp..
✓ Fetched repository from 2 seed(s)
✓ Synced with 2 node(s)
```

Since the `threshold` is set to `2` it's necessary for Alice to also
accept this change:

``` ~alice
$ rad sync
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6MkvVv…Z1Ct4tD..
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6MkuPZ…xEuaPUp..
✓ Fetched repository from 2 seed(s)
✓ Nothing to announce, already in sync with network (see `rad sync status`)
$ rad id list
╭────────────────────────────────────────────────────────────────────────────────╮
│ ●   ID        Title              Author                     Status     Created │
├────────────────────────────────────────────────────────────────────────────────┤
│ ●   3cd3c7f   Add Eve            bob      z6Mkt67…v4N1tRk   active     now     │
│ ●   069e7d5   Add Bob            alice    (you)             accepted   now     │
│ ●   0656c21   Initial revision   alice    (you)             accepted   now     │
╰────────────────────────────────────────────────────────────────────────────────╯
$ rad inspect rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --sigrefs
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

At this point Alice will want to fetch so that she can get Eve's fork:

``` ~alice
$ rad sync --timeout 3
✗ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6MkvVv…Z1Ct4tD.. error: missing required refs: ["refs/namespaces/z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z/refs/rad/sigrefs"]
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6MkuPZ…xEuaPUp..
✓ Fetched repository from 1 seed(s)
✓ Synced with 1 node(s)
! Seed z6MkvVv69U1HGuN6yUd8RiYE8py6QYRzuQoG45xSpZ1Ct4tD timed out..
$ rad inspect rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --sigrefs
z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi 1f716870f890be0c13fdd0af9f527af849fec792
z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk c40018821dc1b41cad75e91e0c9d00827e815324
z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z 95cd447c57de8d232c6154f5dba0451aa593520e
```

Note that `z6MkvVv69U1HGuN6yUd8RiYE8py6QYRzuQoG45xSpZ1Ct4tD` fails to
fetch since it did not have Eve's fork, and similarly, it could not
fetch from Alice and times out for the same reason. However, Alice was
able to successfully fetch from `z6MkvVv…Z1Ct4tD`, since it did have
Eve's fork.

Since the network is eventually consistent, if Eve decides to `sync`
(this could also happen through a transient announcement), then we can
see that both seeds are `synced`:

``` ~eve
$ rad sync
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6MkvVv…Z1Ct4tD..
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6MkuPZ…xEuaPUp..
✓ Fetched repository from 2 seed(s)
✓ Synced with 1 node(s)
$ rad sync status
╭────────────────────────────────────────────────────────────────────────────╮
│ ●   Node                            Address   Status   Tip       Timestamp │
├────────────────────────────────────────────────────────────────────────────┤
│ ●   eve           (you)                                95cd447   now       │
│ ●   distrustful   z6MkvVv…Z1Ct4tD             synced   95cd447   now       │
│ ●   seed          z6MkuPZ…xEuaPUp             synced   95cd447   now       │
╰────────────────────────────────────────────────────────────────────────────╯
```
