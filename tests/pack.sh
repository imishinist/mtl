#!/bin/bash

. $(dirname $0)/common.inc

cd $(setup_new case1)

$MTL local build > /dev/null
objects=$(find .mtl/objects -type f | wc -l | awk '{print $1}')

$MTL pack
diff <(find .mtl/objects -type f | wc -l | awk '{print $1}') <(echo 0)
diff <($MTL tool redb | wc -l | awk '{print $1}') <(echo $objects)

# pack multiple times
$MTL pack
diff <(find .mtl/objects -type f | wc -l | awk '{print $1}') <(echo 0)
diff <($MTL tool redb | wc -l | awk '{print $1}') <(echo $objects)

# after packed

## cat-object
diff <($MTL cat-object HEAD) <(cat <<EOF
file	d447b1ea40e6988b	README
tree	188acf4cce004363	dir1
tree	ba35f09b9bff44c1	dir2
file	ec3cea290f9e42e8	file1
file	56c2402c3bf24293	file2
file	e8383ee34f1c57f5	main.c
tree	f015d1f89f0287bf	z1
EOF
)

## print-tree
diff -u <($MTL print-tree) <(cat <<EOF | perl -pe 's/^(file|tree) ([a-z0-9]{16}) (.*)$/\1 \2\t\3/'
tree 99f9d6592fc5edec .
file d447b1ea40e6988b README
tree 188acf4cce004363 dir1/
file 83e38dfac6ad32cd dir1/file1
tree ba35f09b9bff44c1 dir2/
file e8ec1f907115a249 dir2/file1
file ec3cea290f9e42e8 file1
file 56c2402c3bf24293 file2
file e8383ee34f1c57f5 main.c
tree f015d1f89f0287bf z1/
file 2f31af8ed6c71ce5 z1/file
EOF
)

## gc

$MTL local build >/dev/null
$MTL gc >/dev/null
$MTL pack
diff <(find .mtl/objects -type f | wc -l | awk '{print $1}') <(echo 0)

$MTL local build --hidden >/dev/null
$MTL pack
$MTL gc >/dev/null

diff <(find .mtl/objects -type f | wc -l | awk '{print $1}') <(echo 0)