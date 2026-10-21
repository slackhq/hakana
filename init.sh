#!/bin/bash
# initialises Git for builds
set -euo pipefail

cd "$(dirname "$0")"

git submodule init
git submodule update
cd third-party/hhvm

# Check the actual patch, since unrelated edits do not mean it is applied.
if git apply --reverse --check ../../hhvm-patch.diff 2>/dev/null; then
    echo "HHVM patch already applied"
else
    git apply --check ../../hhvm-patch.diff
    git apply ../../hhvm-patch.diff
    echo "HHVM patch applied successfully"
fi
