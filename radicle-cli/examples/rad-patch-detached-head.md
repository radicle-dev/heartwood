To ensure that we can handle diverse workflows, we also allow patches to be
opened when we're in the infamous 'detached HEAD' state.

First, we will enter this state by using `git checkout` on a commit object:

``` (stderr) RAD_HINT=1
$ git checkout f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354
Note: switching to 'f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354'.

You are in 'detached HEAD' state. You can look around, make experimental
changes and commit them, and you can discard any commits you make in this
state without impacting any branches by switching back to a branch.

If you want to create a new branch to retain commits you create, you may
do so (now or later) by using -c with the switch command. Example:

  git switch -c <new-branch-name>

Or undo this operation with:

  git switch -

Turn off this advice by setting config variable advice.detachedHead to false

HEAD is now at f2de534 Second commit
```

Now, we can create a commit on top of this and create a patch, as usual:

``` (stderr) RAD_HINT=1
$ git commit -a -m "Add things" -q --allow-empty
$ git push -o patch.message="Add things #1" -o patch.message="See commits for details." rad HEAD:refs/patches
✓ Patch a183e324b82e94c548eb43b7acb7c7d92ebe7761 opened
hint: offline push, your node is not running
hint: to sync with the network, run `rad node start`
To rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

Note that there will be no upstream branch, since we did not have a branch to
set an upstream for in the first place!
