#!/bin/bash

. $(dirname $0)/common.inc

cd $(setup_new case1)

$MTL local build >/dev/null

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

# max-depth
diff -u <($MTL print-tree --max-depth 1) <(cat <<EOF | perl -pe 's/^(file|tree) ([a-z0-9]{16}) (.*)$/\1 \2\t\3/'
tree 99f9d6592fc5edec .
file d447b1ea40e6988b README
tree 188acf4cce004363 dir1/
tree ba35f09b9bff44c1 dir2/
file ec3cea290f9e42e8 file1
file 56c2402c3bf24293 file2
file e8383ee34f1c57f5 main.c
tree f015d1f89f0287bf z1/
EOF
)


# tree
diff -u <($MTL print-tree -t tree) <(cat <<EOF | perl -pe 's/^(file|tree) ([a-z0-9]{16}) (.*)$/\1 \2\t\3/'
tree 99f9d6592fc5edec .
tree 188acf4cce004363 dir1/
tree ba35f09b9bff44c1 dir2/
tree f015d1f89f0287bf z1/
EOF
)

# file
diff -u <($MTL print-tree -t file) <(cat <<EOF | perl -pe 's/^(file|tree) ([a-z0-9]{16}) (.*)$/\1 \2\t\3/'
tree 99f9d6592fc5edec .
file d447b1ea40e6988b README
file 83e38dfac6ad32cd dir1/file1
file e8ec1f907115a249 dir2/file1
file ec3cea290f9e42e8 file1
file 56c2402c3bf24293 file2
file e8383ee34f1c57f5 main.c
file 2f31af8ed6c71ce5 z1/file
EOF
)

# root
$MTL local build --hidden >/dev/null
diff -u <($MTL print-tree -r 32dbd98251e9a916) <(cat <<EOF | perl -pe 's/^(file|tree) ([a-z0-9]{16}) (.*)$/\1 \2\t\3/'
tree 32dbd98251e9a916 .
file 7f20afdd73eeb0a3 .ignore
file 2f31af8ed6c71ce5 file
EOF
)
