The `rad job` command lets you manage job COBs. Let's first checkout the
`heartwood` repository:

```
$ rad checkout rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2
✓ Repository checkout successful under ./heartwood
$ cd heartwood
```

Using the `rad job` (or `rad job list`) command we can see that there are
currently no jobs listed:

```
$ rad job
Nothing to show.
```

Let's create a job to represent a new CI run. We check what the current `HEAD`
of the repository is, and use the `rad job trigger` to start a fresh job for
that commit:

```
$ git rev-parse HEAD
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354
$ rad job trigger f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354
╭──────────────────────────────────────────────────╮
│ Job     fbbda2447c30ebbab9b746498cd41a383ff05225 │
│ Commit  f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354 │
│ State   fresh                                    │
╰──────────────────────────────────────────────────╯
```

Let's check the list again, and we should we see our fresh job there:

```
$ rad job
╭───────────────────────────────╮
│ ●   ID        Commit    State │
├───────────────────────────────┤
│ ●   fbbda24   f2de534   fresh │
╰───────────────────────────────╯
```

From there we can start a new job, assigning an arbitrary identifier `xyzzy`,
which would usually from the CI system that is running the job:

```
$ rad job start fbbda2447c30ebbab9b746498cd41a383ff05225 xyzzy
```

Checking the job again, we can now see that the job is `running`:

```
$ rad job
╭─────────────────────────────────╮
│ ●   ID        Commit    State   │
├─────────────────────────────────┤
│ ●   fbbda24   f2de534   running │
╰─────────────────────────────────╯
$ rad job show fbbda2447c30ebbab9b746498cd41a383ff05225
╭──────────────────────────────────────────────────╮
│ Job     fbbda2447c30ebbab9b746498cd41a383ff05225 │
│ Commit  f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354 │
│ State   running                                  │
│ Run ID  xyzzy                                    │
╰──────────────────────────────────────────────────╯
```

When a job has finished, we can mark it as done -- either with a `--success` or
`--failed` flag -- using the `rad job finish` command:

```
$ rad job finish --success fbbda2447c30ebbab9b746498cd41a383ff05225
$ rad job
╭───────────────────────────────────╮
│ ●   ID        Commit    State     │
├───────────────────────────────────┤
│ ●   fbbda24   f2de534   succeeded │
╰───────────────────────────────────╯
$ rad job show fbbda2447c30ebbab9b746498cd41a383ff05225
╭──────────────────────────────────────────────────╮
│ Job     fbbda2447c30ebbab9b746498cd41a383ff05225 │
│ Commit  f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354 │
│ State   succeeded                                │
│ Run ID  xyzzy                                    │
╰──────────────────────────────────────────────────╯
```
