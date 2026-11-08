#!/bin/bash
# initialises Git for builds
set -euo pipefail

cd "$(dirname "$0")"

git submodule init
git submodule update
