#!/bin/bash

. $(dirname $0)/common.inc

cd $(setup_new case1)

$MTL local build > /dev/null
objects=$(find .mtl/objects -type f | wc -l | awk '{print $1}')

$MTL pack
diff <(find .mtl/objects -type f | wc -l | awk '{print $1}') <(echo 0)
diff <($MTL tool redb .mtl/pack/packed.redb | wc -l | awk '{print $1}') <(echo $objects)