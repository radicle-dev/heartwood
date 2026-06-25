To ensure that we can handle diverse workflows, we also allow patches to be
opened when we're in the infamous 'detached HEAD' state.

First, we will enter this state by using `git checkout` on a commit object:

``` (stderr) RAD_HINT=1
$ git checkout 4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927
Note: switching to '4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927'.

You are in 'detached HEAD' state. You can look around, make experimental
changes and commit them, and you can discard any commits you make in this
state without impacting any branches by switching back to a branch.

If you want to create a new branch to retain commits you create, you may
do so (now or later) by using -c with the switch command. Example:

  git switch -c <new-branch-name>

Or undo this operation with:

  git switch -

Turn off this advice by setting config variable advice.detachedHead to false

HEAD is now at 4c66f0e Second commit
```

Now, we can create a commit on top of this and create a patch, as usual:

``` (stderr) RAD_HINT=1
$ git commit -a -m "Add things" -q --allow-empty
$ git push -o patch.message="Add things #1" -o patch.message="See commits for details." rad HEAD:refs/patches
✓ Patch 6c2fe32a2f0659721e51a298250fbfb7b3081a52 opened
hint: offline push, your node is not running
hint: to sync with the network, run `rad node start`
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

Note that there will be no upstream branch, since we did not have a branch to
set an upstream for in the first place!
