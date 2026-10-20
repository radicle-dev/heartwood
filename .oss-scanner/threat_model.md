<!-- Free text; the scanner reads it before it starts. -->
# Threat model

## What this project does and where untrusted input enters
- e.g. "a compression library; we assume all input is untrusted"

## Components that matter most / least
- e.g. "the decoder matters; the compressor is less important but still in scope; the contrib/ directory is third-party and out of scope"

## How to exercise it
- e.g. "examples/ contains small drivers; test/ has the regression corpus"

## How you rate severity
- e.g. "any buffer overflow, use-after-free, or double free is high+ at a minimum. If the overflow is controlled and may lead to RCE then it should be critical. All DoS is medium."

## Anything to leave alone
- e.g. "do not report unaligned access in the SIMD code paths on x86"
