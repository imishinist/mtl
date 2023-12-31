#!/bin/bash

. $(dirname $0)/common.inc

cd $(setup_new case1)

$MTL local build >/dev/null
$MTL local build --hidden >/dev/null

diff -u <($MTL diff 99f9d6592fc5edec 6b1d722afb0c117d) <(cat <<EOF
-/+ tree/tree	99f9d6592fc5edec/6b1d722afb0c117d	.
 /+     /file	                /f3c610f214152e9f	.ignore
-/+ tree/tree	f015d1f89f0287bf/32dbd98251e9a916	z1
 /+     /file	                /7f20afdd73eeb0a3	z1/.ignore
EOF
)

diff -u <($MTL diff 6b1d722afb0c117d 99f9d6592fc5edec) <(cat <<EOF
-/+ tree/tree	6b1d722afb0c117d/99f9d6592fc5edec	.
-/  file/    	f3c610f214152e9f/                	.ignore
-/+ tree/tree	32dbd98251e9a916/f015d1f89f0287bf	z1
-/  file/    	7f20afdd73eeb0a3/                	z1/.ignore
EOF
)
