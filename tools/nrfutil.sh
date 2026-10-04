#!/usr/bin/env bash
# Classic pc-nrfutil 6.1.7 (needs Python < 3.11, unfree in nixpkgs) in a
# throwaway container. The image is built once and cached as `swet102-nrfutil`.
set -euo pipefail
if ! docker image inspect swet102-nrfutil >/dev/null 2>&1; then
  printf 'FROM python:3.10-slim\nRUN pip install --no-cache-dir nrfutil==6.1.7\nENTRYPOINT ["nrfutil"]\n' \
    | docker build -q -t swet102-nrfutil - >/dev/null
fi
exec docker run --rm -u "$(id -u):$(id -g)" -v "$PWD:$PWD" -w "$PWD" swet102-nrfutil "$@"
