#!/bin/sh

set -e

DIRNAME=$1

BUILD_DIR=$DIRNAME/build

rm -rf $BUILD_DIR
mkdir -p $BUILD_DIR
cd $BUILD_DIR

git clone git@github.com:jet-lab/jet-governance.git
cd jet-governance
git checkout 6b7139e
git apply $DIRNAME/patch
cargo build-bpf
