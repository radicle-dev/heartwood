# Security Policy

Please note that <https://radicle.dev/.well-known/security.txt> is
applicable to this repository. If you suspect that the information
available there is in conflict with information in this file, then
this file should take priority.

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 5.1.x   | :white_check_mark: |
| 5.0.x   | :x:                |
| 4.0.x   | :white_check_mark: |
| < 4.0   | :x:                |

## Reporting a Security Vulnerability

You may report security vulnerabilities via:
 1. Radicle (as COB of type `xyz.radicle.issue`)
 2. <mailto:security@radicle.dev>

In both cases, you may choose to encrypt your report using
<https://age-encryption.org>. Note that it is possible to convert
Decantralized Identifiers of the form `did:key:z6Mk…` to age
recipients. For example, see <https://radicle.dev/recipients.txt>.
In general, it is not safe to assume that everyone will be able to
decrypt age messages using their Radicle key. For example, they might
be using a hardware security key that is incompatible with age.
You should expect an unencrypted reply in case your report cannot be
decrypted.

If you are reporting by creating an issue on Radicle, please use
`age --armor`.

Please use your own judgement to decide which method of reporting is
suitable. When in doubt, feel free to ask for advice.
