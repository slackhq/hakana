#!/bin/bash
# initialises Git for builds

git submodule init
git submodule update
cd third-party/hhvm
git apply ../../hhvm-patch.diff