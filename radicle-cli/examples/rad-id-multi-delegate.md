``` ~alice
$ rad id update --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --title "Add Bob" --description "" --threshold 2 --delegate did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --no-confirm -q
069e7d58faa9a7473d27f5510d676af33282796f
```

``` ~bob
$ rad watch --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --node z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z -r 'refs/rad/sigrefs' -t c9a828fc2fb01f893d6e6e9e17b9092dea2b3aba -i 500 --timeout 5000
$ rad sync --fetch rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from the network, found 1 potential seed(s).
✓ Target met: 1 replica(s)
🌱 Fetched from z6MknSL…StBU8Vi
$ rad id --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
╭────────────────────────────────────────────────────────────────────────────────╮
│ ●   ID        Title              Author                     Status     Created │
├────────────────────────────────────────────────────────────────────────────────┤
│ ●   069e7d5   Add Bob            alice    z6MknSL…StBU8Vi   accepted   now     │
│ ●   0656c21   Initial revision   alice    z6MknSL…StBU8Vi   accepted   now     │
╰────────────────────────────────────────────────────────────────────────────────╯
$ rad inspect rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --sigrefs
z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi [..]
z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk [..]
z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z [..]
$ rad inspect rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --delegates
did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi (alice)
did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk (bob)
$ rad id update --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --title "Add Eve" --description "" --delegate did:key:z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z --no-confirm
✓ Identity revision 3cd3c7f9900de0fcb19705856a7cc339a38fb0b3 created
╭────────────────────────────────────────────────────────────────────────╮
│ Title    Add Eve                                                       │
│ Revision 3cd3c7f9900de0fcb19705856a7cc339a38fb0b3                      │
│ Blob     74581605d1f75396c331487a10ca61c4815ed685                      │
│ Author   did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk      │
│ State    active                                                        │
│ Quorum   no                                                            │
├────────────────────────────────────────────────────────────────────────┤
│ ✓ did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk bob   (you) │
│ ? did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi alice       │
╰────────────────────────────────────────────────────────────────────────╯

@@ -1,14 +1,15 @@
 {
   "payload": {
     "xyz.radicle.project": {
       "defaultBranch": "master",
       "description": "Radicle Heartwood Protocol & Stack",
       "name": "heartwood"
     }
   },
   "delegates": [
     "did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi",
-    "did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk"
+    "did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk",
+    "did:key:z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z"
   ],
   "threshold": 2
 }
```

``` ~alice
$ rad sync --fetch rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from the network, found 2 potential seed(s).
✓ Target met: 2 replica(s)
🌱 Fetched from z6Mkux1…nVhib7Z
🌱 Fetched from z6Mkt67…v4N1tRk
$ rad inspect rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --delegates
did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi (alice)
did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk (bob)
$ rad id accept 3cd3c7f9900de0fcb19705856a7cc339a38fb0b3 --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --no-confirm
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
$ rad inspect rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --delegates
did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi (alice)
did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk (bob)
did:key:z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z (eve)
```

``` ~alice
$ rad id update --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --title "Make private" --description "" --visibility private --no-confirm -q
e6bf10593b78384eb2b281cbb18a605668a6d1f7
```

We can list all revisions:

``` ~alice
$ rad id list
╭────────────────────────────────────────────────────────────────────────────────╮
│ ●   ID        Title              Author                     Status     Created │
├────────────────────────────────────────────────────────────────────────────────┤
│ ●   e6bf105   Make private       alice    (you)             active     now     │
│ ●   3cd3c7f   Add Eve            bob      z6Mkt67…v4N1tRk   accepted   now     │
│ ●   069e7d5   Add Bob            alice    (you)             accepted   now     │
│ ●   0656c21   Initial revision   alice    (you)             accepted   now     │
╰────────────────────────────────────────────────────────────────────────────────╯
```

Despite being a delegate, Bob can't edit or redact Alice's revision:

``` ~bob (fail)
$ rad id redact e6bf10593b78384eb2b281cbb18a605668a6d1f7
[..]
```
``` ~bob (fail)
$ rad id edit --title "Boo!" --description "Boo!" e6bf10593b78384eb2b281cbb18a605668a6d1f7
[..]
```

Alice can edit:

``` ~alice
$ rad id edit --title "Make private" --description "Privacy is cool." e6bf10593b78384eb2b281cbb18a605668a6d1f7
✓ Revision e6bf10593b78384eb2b281cbb18a605668a6d1f7 edited
$ rad id show e6bf10593b78384eb2b281cbb18a605668a6d1f7
╭────────────────────────────────────────────────────────────────────────╮
│ Title    Make private                                                  │
│ Revision e6bf10593b78384eb2b281cbb18a605668a6d1f7                      │
│ Blob     c533865b2846ca6c5b4436ec6872257293380c3b                      │
│ Author   did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi      │
│ State    active                                                        │
│ Quorum   no                                                            │
│                                                                        │
│ Privacy is cool.                                                       │
├────────────────────────────────────────────────────────────────────────┤
│ ✓ did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi alice (you) │
│ ? did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk bob         │
│ ? did:key:z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z eve         │
╰────────────────────────────────────────────────────────────────────────╯

@@ -1,15 +1,18 @@
 {
   "payload": {
     "xyz.radicle.project": {
       "defaultBranch": "master",
       "description": "Radicle Heartwood Protocol & Stack",
       "name": "heartwood"
     }
   },
   "delegates": [
     "did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi",
     "did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk",
     "did:key:z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z"
   ],
-  "threshold": 2
+  "threshold": 2,
+  "visibility": {
+    "type": "private"
+  }
 }
```

And she can redact her revision:

``` ~alice
$ rad id redact e6bf10593b78384eb2b281cbb18a605668a6d1f7
✓ Revision e6bf10593b78384eb2b281cbb18a605668a6d1f7 redacted
```
``` ~alice (fail)
$ rad id show e6bf10593b78384eb2b281cbb18a605668a6d1f7
✗ Error: revision `e6bf10593b78384eb2b281cbb18a605668a6d1f7` not found
```

Finally, Alice can also propose to remove Bob:
``` ~alice
$ rad id update --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --title "Remove Bob" --description "" --rescind did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --no-confirm
✓ Identity revision 8ba242a80bc1181f41f9ea7a19286038c7948994 created
╭────────────────────────────────────────────────────────────────────────╮
│ Title    Remove Bob                                                    │
│ Revision 8ba242a80bc1181f41f9ea7a19286038c7948994                      │
│ Blob     254d62de237117e7d7b9ceff85c47f5e3b610c1e                      │
│ Author   did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi      │
│ State    active                                                        │
│ Quorum   no                                                            │
├────────────────────────────────────────────────────────────────────────┤
│ ✓ did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi alice (you) │
│ ? did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk bob         │
│ ? did:key:z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z eve         │
╰────────────────────────────────────────────────────────────────────────╯

@@ -1,15 +1,14 @@
 {
   "payload": {
     "xyz.radicle.project": {
       "defaultBranch": "master",
       "description": "Radicle Heartwood Protocol & Stack",
       "name": "heartwood"
     }
   },
   "delegates": [
     "did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi",
-    "did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk",
     "did:key:z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z"
   ],
   "threshold": 2
 }
```
