#!/bin/bash

. $(dirname $0)/common.inc

cd $(setup_new case1)

diff <($MTL local list | sort -k2) <(cat <<EOF
tree .
file README
tree dir1
file dir1/file1
tree dir2
file dir2/file1
file file1
file file2
file main.c
tree z1
file z1/file
EOF
)

diff <($MTL local list -i <(echo 'README') | sort -k2) <(cat <<EOF
tree .
file README
EOF
)

diff <(echo 'README' | $MTL local list -i - | sort -k2) <(cat <<EOF
tree .
file README
EOF
)

diff <($MTL local list --hidden | sort -k2) <(cat <<EOF
tree .
file .ignore
file README
tree dir1
file dir1/file1
tree dir2
file dir2/file1
file file1
file file2
file main.c
tree z1
file z1/.ignore
file z1/file
EOF
)