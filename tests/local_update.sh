#!/bin/bash

. $(dirname $0)/common.inc

cd $(setup_new case1)

$MTL local build --hidden >/dev/null

echo "dummy data" >> README
echo "Hello" >> z1/file

$MTL local update --hidden z1 >/dev/null
$MTL ref save update-root HEAD >/dev/null

$MTL local build --hidden >/dev/null

diff -u <($MTL rev-parse HEAD:z1) <($MTL rev-parse update-root:z1)

# add file
echo "hello" >> z1/file2

$MTL local update --hidden z1 >/dev/null
diff -u <($MTL cat-object HEAD:z1) <(cat .mtl/objects/31/32b2cc5f6bcb59)

cd - >/dev/null
cd $(setup_new case2)

$MTL local build --hidden >/dev/null
echo "dummy data" >> README
echo "Hello" >> a1/3ba7983e72764940

$MTL local update --hidden a1 >/dev/null
$MTL ref save update-root HEAD >/dev/null

$MTL local build --hidden >/dev/null
diff -u <($MTL rev-parse HEAD:a1) <($MTL rev-parse update-root:a1)
