#!/bin/bash

set -euo pipefail

cd "$(dirname "$0")/../"

usage() {
  echo "Usage: $0 <revision_a> <revision_b>"
  exit 1
}

trap "echo 'An error occurred. Exiting.'" ERR

[ $# -ne 2 ] && usage

readonly revision_a=$1
readonly revision_b=$2
readonly hash_a=$(git rev-parse --short $revision_a)
readonly hash_b=$(git rev-parse --short $revision_b)

echo "Comparing $revision_a($hash_a) and $revision_b($hash_b)"
echo

echo "Building $revision_a($hash_a)"
path_a=$(./tools/build-by-revision.sh $revision_a 2>/dev/null | grep "Generated" | cut -d" " -f3)

echo "Building $revision_b($hash_b)"
path_b=$(./tools/build-by-revision.sh $revision_b 2>/dev/null | grep "Generated" | cut -d" " -f3)

echo "Compare $path_a and $path_b"

diff -u \
  <($(pwd)/$path_a local build) \
  <($(pwd)/$path_b local build)

diff -u \
  <($(pwd)/$path_a print-tree) \
  <($(pwd)/$path_b print-tree)

echo "OK"
