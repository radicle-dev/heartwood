With the `rad job` command lets you manage job COBs.

```
$ rad checkout rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
✓ Repository checkout successful under ./heartwood
$ cd heartwood
```

Initially we have not jobs to list.

```
$ rad job
Nothing to show.
```

Same with the full command.

```
$ rad job list
Nothing to show.
```

Create a job COB to represent a CI run.

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

We can now list the job.

```
$ rad job
╭────────────────────────────────────────────────────────────────╮
│ ●   ID                                         Commit    State │
├────────────────────────────────────────────────────────────────┤
│ ●   fbbda2447c30ebbab9b746498cd41a383ff05225   f2de534   fresh │
╰────────────────────────────────────────────────────────────────╯
```

Mark the job as started. `xyzzy` is the identifier assigned by the
automation that runs the job, such as a remote CI system.

```
$ rad job start fbbda2447c30ebbab9b746498cd41a383ff05225 xyzzy
```

FIXME: the above should probably output something useful

It's now marked as running.

```
$ rad job
╭──────────────────────────────────────────────────────────────────╮
│ ●   ID                                         Commit    State   │
├──────────────────────────────────────────────────────────────────┤
│ ●   fbbda2447c30ebbab9b746498cd41a383ff05225   f2de534   running │
╰──────────────────────────────────────────────────────────────────╯
$ rad job show fbbda2447c30ebbab9b746498cd41a383ff05225
╭──────────────────────────────────────────────────╮
│ Job     fbbda2447c30ebbab9b746498cd41a383ff05225 │
│ Commit  f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354 │
│ State   running                                  │
│ Run ID  xyzzy                                    │
╰──────────────────────────────────────────────────╯
```

Mark job as finished successfully

```
$ rad job finish --success fbbda2447c30ebbab9b746498cd41a383ff05225
$ rad job
╭────────────────────────────────────────────────────────────────────╮
│ ●   ID                                         Commit    State     │
├────────────────────────────────────────────────────────────────────┤
│ ●   fbbda2447c30ebbab9b746498cd41a383ff05225   f2de534   succeeded │
╰────────────────────────────────────────────────────────────────────╯
$ rad job show fbbda2447c30ebbab9b746498cd41a383ff05225
╭──────────────────────────────────────────────────╮
│ Job     fbbda2447c30ebbab9b746498cd41a383ff05225 │
│ Commit  f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354 │
│ State   succeeded                                │
│ Run ID  xyzzy                                    │
╰──────────────────────────────────────────────────╯
```
