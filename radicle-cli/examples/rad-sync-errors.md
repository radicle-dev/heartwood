```
$ rad issue open --title "Test `rad sync`" --description "Check that it works" -q --no-announce
$ rad sync status
╭──────────────────────────────────────────────────────────────────────────────────────────────╮
│ ●   Node                      Address                      Status        Tip       Timestamp │
├──────────────────────────────────────────────────────────────────────────────────────────────┤
│ ●   alice   (you)             alice.radicle.example:8776   unannounced   0949fa4   now       │
│ ●   bob     z6Mkt67…v4N1tRk   bob.radicle.example:8776     out-of-sync   f209c9f   now       │
│ ●   eve     z6Mkux1…nVhib7Z   eve.radicle.example:8776     out-of-sync   f209c9f   now       │
╰──────────────────────────────────────────────────────────────────────────────────────────────╯
```

```
$ rad sync --announce --timeout 3
✓ Synced with 1 node(s)
$ rad sync status
╭──────────────────────────────────────────────────────────────────────────────────────────────╮
│ ●   Node                      Address                      Status        Tip       Timestamp │
├──────────────────────────────────────────────────────────────────────────────────────────────┤
│ ●   alice   (you)             alice.radicle.example:8776                 0949fa4   now       │
│ ●   bob     z6Mkt67…v4N1tRk   bob.radicle.example:8776     synced        0949fa4   now       │
│ ●   eve     z6Mkux1…nVhib7Z   eve.radicle.example:8776     out-of-sync   f209c9f   now       │
╰──────────────────────────────────────────────────────────────────────────────────────────────╯
```

```
$ rad sync --announce --timeout 1
✗ Found 1 seed(s)..
✗ Error: all seeds timed out
```
