The `rad patch review` command must provide an option or a review message, since
a review must have a verdict or a review summary:

``` (fail)
$ rad patch review 70a5938 --no-message --no-announce
✗ Error: expected one of `--accept` or `--reject`, or supply a review message
```

This time we will supply a message:

```
$ rad patch review 70a5938 -m "Looks reasonable, no strong opinion." --no-announce
✓ Patch 70a5938 reviewed
```

Submitting a second review on the same revision by the same author is ignored.
Currently, there is no way to edit a review from the CLI.

Note that the review command will still succeed, but with no effect:

```
$ rad patch review 70a5938 -m "Changed my mind." --no-announce
✓ Patch 70a5938 reviewed
```
