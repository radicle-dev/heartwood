To display a repository's identifier, or *RID*, you may use the `rad inspect`
command from inside a working copy:

```
$ rad inspect
rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2
```

As a shorthand, you can also simply use `rad .`:

```
$ rad .
rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2
```

It's also possible to display all of the repository's git references:

```
$ rad inspect --refs
z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
└── refs
    ├── cobs
    │   └── xyz.radicle.id
    │       └── eeb8b44890570ccf85db7f3cb2a475100a27408a
    ├── heads
    │   └── master
    └── rad
        ├── id
        ├── root
        └── sigrefs
```

And sigrefs:

```
$ rad inspect --sigrefs
z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi 7c1445cd018b1b0f51e0d815c3c03b289140eafa
```

Or display the repository identity's payload and delegates:

```
$ rad inspect --payload
{
  "xyz.radicle.project": {
    "defaultBranch": "master",
    "description": "Radicle Heartwood Protocol & Stack",
    "name": "heartwood"
  }
}
$ rad inspect --delegates
did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi (alice)
```

Finally, the `--history` flag allows you to examine the identity document's
history:

```
$ rad inspect --history
commit eeb8b44890570ccf85db7f3cb2a475100a27408a
blob   b38d81ee99d880461a3b7b3502e5d1556e440ef3
date   Thu, 15 Dec 2022 17:28:04 +0000

    Initialize identity

 {
   "version": 2,
   "payload": {
     "xyz.radicle.project": {
       "defaultBranch": "master",
       "description": "Radicle Heartwood Protocol & Stack",
       "name": "heartwood"
     }
   },
   "delegates": [
     "did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi"
   ],
   "canonicalRefs": {
     "rules": {
       "refs/heads/master": {
         "allow": "delegates",
         "threshold": 1
       }
     }
   }
 }

```

The identity document is the metadata associated with a repository, that is
only changeable by delegates.
