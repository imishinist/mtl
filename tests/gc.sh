#!/bin/bash

. $(dirname $0)/common.inc

cd $(setup_new case1)

$MTL local build >/dev/null

diff <($MTL gc --dry | wc -l | awk '{print $1}') <(echo 1)
diff <(find .mtl/objects -type f | wc -l) <(find . -type d -not -path "*.mtl*" | wc -l)

$MTL local build --hidden >/dev/null
# diff
#   - root
#   - z1
diff <($MTL gc --dry | wc -l | awk '{print $1}') <(echo 3)

$MTL gc >/dev/null
diff <(find .mtl/objects -type f | wc -l) <(find . -type d -not -path "*.mtl*" | wc -l)
