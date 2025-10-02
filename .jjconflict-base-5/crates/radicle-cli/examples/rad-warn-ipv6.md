```
$ rad config push preferredSeeds z6MkvUJtYD9dHDJfpevWRT98mzDDpdAtmUjwyDSkyqksUr7C@2001:db8::1:8776
z6MkvUJtYD9dHDJfpevWRT98mzDDpdAtmUjwyDSkyqksUr7C@2001:db8::1:8776
$ rad config push node.connect z6MkvUJtYD9dHDJfpevWRT98mzDDpdAtmUjwyDSkyqksUr7C@2001:db8::2:8776
z6MkvUJtYD9dHDJfpevWRT98mzDDpdAtmUjwyDSkyqksUr7C@2001:db8::2:8776
$ rad config push node.externalAddresses 2001:db8::3:8776
2001:db8::3:8776
```

Note the warnings that the above configuration causes when running `rad node status`:

```
$ rad node status
! Warning: Value of configuration option `preferredSeeds` at zero-based index 0 mentions IPv6 address '2001:db8::1' without square brackets. The address format will change, and this address will be rejected in the future. Please edit your configuration file to enclose the IPv6 address in square brackets. Combined with port information it should read '[2001:db8::1]:8776'. Refer to RFC 5926, Sec. 6 as well as RFC 3986, Sec. D.1. and RFC 2732, Sec. 2.
! Warning: Value of configuration option `node.connect` at zero-based index 0 mentions IPv6 address '2001:db8::2' without square brackets. The address format will change, and this address will be rejected in the future. Please edit your configuration file to enclose the IPv6 address in square brackets. Combined with port information it should read '[2001:db8::2]:8776'. Refer to RFC 5926, Sec. 6 as well as RFC 3986, Sec. D.1. and RFC 2732, Sec. 2.
! Warning: Value of configuration option `node.externalAddresses` at zero-based index 0 mentions IPv6 address '2001:db8::3' without square brackets. The address format will change, and this address will be rejected in the future. Please edit your configuration file to enclose the IPv6 address in square brackets. Combined with port information it should read '[2001:db8::3]:8776'. Refer to RFC 5926, Sec. 6 as well as RFC 3986, Sec. D.1. and RFC 2732, Sec. 2.
Node is stopped.
To start it, run `rad node start`.
```
