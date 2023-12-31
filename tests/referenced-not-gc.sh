#!/bin/bash

. $(dirname $0)/common.inc

cd $(setup_new case1)

$MTL local build --hidden >/dev/null
$MTL ref save root >/dev/null

$MTL local build >/dev/null

# exist ref
diff -u <($MTL gc | wc -l | awk '{print $1}') <(echo 1)

$MTL ref delete root >/dev/null
diff -u <($MTL gc | wc -l | awk '{print $1}') <(echo 3)