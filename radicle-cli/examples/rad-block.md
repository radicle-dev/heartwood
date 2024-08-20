When using an open seeding policy, it can be useful to block individual
repositories from being seeded.

For instance, if our default policy is to seed, any unknown repository will
have its policy set to allow seeding:
```
$ rad inspect rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 --policy
Repository rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 is being seeded with scope `all`
```

Since there is no policy specific to this repository, there's nothing to be
removed.

```
$ rad seed
No seeding policies to show.
```

But if we wanted to prevent this repository from being seeded, while
allowing all other repositories, we could use `rad block`:

```
$ rad block rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2
✓ Policy for rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 set to 'block'
```

We can see that it is now no longer seeded:

```
$ rad inspect rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 --policy
Repository rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 is not being seeded
```

And a 'block' policy was added:

```
$ rad seed
╭───────────────────────────────────────────────────────────╮
│ Repository                          Name   Policy   Scope │
├───────────────────────────────────────────────────────────┤
│ rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2          block    all   │
╰───────────────────────────────────────────────────────────╯
```

If we want to reverse the blocking of the RID we can use `rad unblock`:

```
$ rad unblock rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2
✓ The 'block' policy for rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 is removed
```

```
$ rad seed
No seeding policies to show.
```
