#!/bin/bash

. $(dirname $0)/common.inc

cd $(setup_new case1)

$MTL local build >/dev/null
$MTL ref save z1 f015d1f89f0287bf >/dev/null

# cat-object
## HEAD
diff -u <($MTL cat-object HEAD) <(cat .mtl/objects/99/f9d6592fc5edec)
## ref-name
diff -u <($MTL cat-object z1) <(cat .mtl/objects/f0/15d1f89f0287bf)

# ref
$MTL ref save root HEAD >/dev/null
$MTL ref list | grep -Eq "^root\s99f9d6592fc5edec$"

# diff
$MTL local build --hidden >/dev/null
$MTL ref save root-hidden HEAD >/dev/null
diff -u <($MTL diff root root-hidden) <(cat <<EOF
-/+ tree/tree	99f9d6592fc5edec/6b1d722afb0c117d	.
 /+     /file	                /f3c610f214152e9f	.ignore
-/+ tree/tree	f015d1f89f0287bf/32dbd98251e9a916	z1
 /+     /file	                /7f20afdd73eeb0a3	z1/.ignore
EOF
)

# print-tree
## HEAD
$MTL local build >/dev/null
$MTL print-tree -r HEAD | grep -Eq "^tree\s99f9d6592fc5edec\s.$"
## ref-name
$MTL print-tree -r z1 | grep -Eq "^tree\sf015d1f89f0287bf\s.$"
