The `rad patch review` command must provide an option or a review message, since
a review must have a verdict or a review summary:

``` (fail)
$ rad patch --no-announce review aa45913 --no-message
✗ Error: expected one of `--accept` or `--reject`, or supply a review message
```

This time we will supply a message:

```
$ rad patch --no-announce review aa45913 -m "Looks reasonable, no strong opinion."
✓ Patch aa45913 reviewed
```

Submitting a second review on the same revision by the same author is ignored.
Currently, there is no way to edit a review from the CLI.

Note that the review command will still succeed, but with no effect:

```
$ rad patch --no-announce review aa45913 -m "Changed my mind."
✓ Patch aa45913 reviewed
```
