Tagging an issue is easy, let's add the `bug` and `good-first-issue` tags to
some issue:

```
$ rad tag 2e8c1bf3fe0532a314778357c886608a966a34bd bug good-first-issue
```

We can now show the issue to check whether those tags were added:

```
$ rad issue show 2e8c1bf3fe0532a314778357c886608a966a34bd
title: flux capacitor underpowered
state: open
tags: [bug, good-first-issue]
assignees: []

Flux capacitor power requirements exceed current supply
```

Untagging an issue is very similar:

```
$ rad untag 2e8c1bf3fe0532a314778357c886608a966a34bd good-first-issue
```

Notice that the `good-first-issue` tag has disappeared:

```
$ rad issue show 2e8c1bf3fe0532a314778357c886608a966a34bd
title: flux capacitor underpowered
state: open
tags: [bug]
assignees: []

Flux capacitor power requirements exceed current supply
```
