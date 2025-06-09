It is possible to use the `rad seed` command to specify multiple RIDs at the
same time, where each repository specified will be fetched (unless `--no-fetch`
is used):

```
$ rad seed rad:z3Rry7rpdWuGpfjPYGzdJKQADsoNW rad:z3zTnCfi6cVSZG8eCGn6AMDypgAPm
✓ Seeding policy updated for rad:z3Rry7rpdWuGpfjPYGzdJKQADsoNW with scope 'all'
Fetching rad:z3Rry7rpdWuGpfjPYGzdJKQADsoNW from the network, found 1 potential seed(s).
✓ Target met: 1 seed(s)
✓ Seeding policy updated for rad:z3zTnCfi6cVSZG8eCGn6AMDypgAPm with scope 'all'
Fetching rad:z3zTnCfi6cVSZG8eCGn6AMDypgAPm from the network, found 1 potential seed(s).
✓ Target met: 1 seed(s)
```
