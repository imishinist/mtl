#!/bin/bash

set -euo pipefail

cd "$(dirname "$0")/../"

usage() {
  echo "Usage: $0 <dir> <revision_a> <revision_b> [hyperfine_params]"
  exit 1
}

trap "echo 'An error occurred. Exiting.'" ERR

[ $# -le 3 ] && usage

readonly dir=$1
readonly revision_a=$2
readonly revision_b=$3
readonly hyperfine_params=${@:4:($#-3)}

echo "$revision_a: $(git rev-parse --short $revision_a)"
echo "$revision_b: $(git rev-parse --short $revision_b)"
echo

path_a=$(./tools/build-by-revision.sh $revision_a 2>/dev/null | grep "Generated" | cut -d" " -f3)
path_b=$(./tools/build-by-revision.sh $revision_b 2>/dev/null | grep "Generated" | cut -d" " -f3)

hyperfine $hyperfine_params \
  "$(pwd)/$path_a local build -c $dir" \
  "$(pwd)/$path_b local build -c $dir"

