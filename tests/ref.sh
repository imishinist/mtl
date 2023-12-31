#!/bin/bash

. $(dirname $0)/common.inc

cd $(setup_new case1)

# empty ref list
diff -u <($MTL ref list) <(echo -n "")
$MTL local build >/dev/null

# root ref list
$MTL ref save root >/dev/null

tmpfile=$(mktemp)
echo -e "root\t99f9d6592fc5edec" > $tmpfile
diff -u <($MTL ref list) $tmpfile

# root and z1 ref list
$MTL ref save z1 f015d1f89f0287bf >/dev/null

tmpfile=$(mktemp)
echo -e "root\t99f9d6592fc5edec" > $tmpfile
echo -e "z1\tf015d1f89f0287bf" >> $tmpfile
diff -u <($MTL ref list) $tmpfile

# root and z1 ref list with hidden
$MTL ref delete root >/dev/null

tmpfile=$(mktemp)
echo -e "z1\tf015d1f89f0287bf" > $tmpfile
diff -u <($MTL ref list) $tmpfile

$MTL ref delete z1 >/dev/null