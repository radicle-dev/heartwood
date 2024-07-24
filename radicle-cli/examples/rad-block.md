When using an open seeding policy, it can be useful to block individual
repositories from being seeded.

For instance, if our default policy is to seed, any unknown repository will
have its policy set to allow seeding:
```
$ rad inspect rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --policy
Repository rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji is being seeded with scope `all`
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
$ rad block rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
✓ Policy for rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji set to 'block'
```

We can see that it is now no longer seeded:

```
$ rad inspect rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --policy
Repository rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji is not being seeded
```

And a 'block' policy was added:

```
$ rad seed
╭───────────────────────────────────────────────────────────╮
│ Repository                          Name   Policy   Scope │
├───────────────────────────────────────────────────────────┤
│ rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji          block    all   │
╰───────────────────────────────────────────────────────────╯
```

If we want to reverse the blocking of the RID we can use `rad unblock`:

```
$ rad unblock rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
✓ The 'block' policy for rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji is removed
```

```
$ rad seed
No seeding policies to show.
```
