Let's say we have a private repo. To make it public, we use the `publish` command:

```
$ rad inspect --visibility
private
$ rad publish
✓ Repository is now public
! Warning: Your node is not running. Start your node with `rad node start` to announce your repository to the network
$ rad inspect --visibility
public
```

If we try to publish again, we get an error:

``` (fail)
$ rad publish
✗ Error: repository is already public
✗ Hint: to announce the repository to the network, run `rad sync --inventory`
```
