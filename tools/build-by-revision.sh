#!/bin/bash

set -eu

usage() {
  echo "Usage: $0 <revision>" >&2
  exit 1
}

build() {
  local revision=$1
  local param=$2

  echo
  echo cargo build --release --bin mtl $param
  cargo build --release --bin mtl $param
  cp target/release/mtl{,-$revision}
  echo "Generated to target/release/mtl-$revision"
  echo
}

git diff --shortstat --exit-code --quiet >/dev/null 2>&1
if [ $? -ne 0 ]; then
  echo "There are uncommitted changes. Please commit or stash them before building." >&2
  exit 1
fi

[ $# -eq 0 ] && usage

revision=$1
params=${2:-""}
hash=$(git rev-parse --short $revision)
tmp_branch=$(head /dev/urandom | openssl dgst -sha1 --binary | xxd -p)


echo "Building revision $revision($hash)..."

current_branch=$(git rev-parse --abbrev-ref HEAD)
if [ "$current_branch" = "HEAD" ]; then
  build $hash "$params"
  exit
fi

git switch $revision -c $tmp_branch
trap "git checkout $current_branch; git branch -D $tmp_branch" EXIT

build $hash "$params"
