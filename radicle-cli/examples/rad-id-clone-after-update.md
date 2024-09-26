``` ~alice
$ rad id update --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --title "Add Bob" --description "" --delegate did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --no-confirm -q
c036c0d89ce26aef3ad7da402157dba16b5163b4
$ rad sync -a
✓ Synced with 1 node(s)
```

``` ~bob
$ rad clone rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
✓ Seeding policy updated for rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji with scope 'all'
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6MknSL…StBU8Vi..
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6MkuPZ…xEuaPUp..
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
$ cd heartwood
```

``` ~bob
$ rad id update --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --title "Add Bob desktop" --description "" --delegate did:key:z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z --no-confirm -q
22167df6ad94fa0c124e709c9dd597f163fa9fa5
```

``` ~alice
$ rad id update --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --title "Add Bob desktop" --description "" --delegate did:key:z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z --no-confirm -q
d14e00e90437e433685a699b874b393b8bc9c2e5
$ rad id
╭──────────────────────────────────────────────────────────────────────╮
│ ●   ID        Title              Author           Status     Created │
├──────────────────────────────────────────────────────────────────────┤
│ ●   d14e00e   Add Bob desktop    alice    (you)   active     now     │
│ ●   c036c0d   Add Bob            alice    (you)   accepted   now     │
│ ●   0656c21   Initial revision   alice    (you)   accepted   now     │
╰──────────────────────────────────────────────────────────────────────╯
```

``` ~bob (fail)
$ rad sync -a
✗ Found 2 seed(s)..
✗ Error: all seeds timed out
```

``` ~alice
$ rad sync
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6MkuPZ…xEuaPUp..
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6Mkt67…v4N1tRk..
✓ Fetched repository from 2 seed(s)
✓ Synced with 2 node(s)
$ rad id
```
