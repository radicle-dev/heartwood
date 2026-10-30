#! /usr/bin/env sh
find build/artifacts -type f -name '*.sha256' -exec cat {} +
