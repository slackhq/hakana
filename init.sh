#!/bin/bash
# initialises Git for builds

git submodule init
git submodule update
cd third-party/hhvm

# Check if patch is already applied by looking for changes or applying it safely
if git diff --quiet; then
    # No changes detected, try to apply the patch
    if git apply ../../hhvm-patch.diff 2>/dev/null; then
        echo "✅ Patch applied successfully"
    else
        echo "ℹ️  Patch may already be applied or not needed"
    fi
else
    echo "ℹ️  Changes already present in hhvm submodule (patch likely applied)"
fi