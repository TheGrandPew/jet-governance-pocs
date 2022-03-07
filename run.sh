#!/bin/sh

ROOT=$( cd ${0%/*} && pwd -P )
SCRIPT_DIR=$ROOT/$1

if [ ! -f "$SCRIPT_DIR/Cargo.toml" ]
then
  echo "usage: $0 <dirname>"
  exit 1
fi

$ROOT/build.sh $SCRIPT_DIR && cd $SCRIPT_DIR && cargo r --release | grep -v "writing bytes"
