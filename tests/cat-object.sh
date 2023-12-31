#!/bin/bash

. $(dirname $0)/common.inc

cd $(setup_new case1)

$MTL local build >/dev/null

diff -u <($MTL cat-object 99f9d6592fc5edec) <(cat .mtl/objects/99/f9d6592fc5edec)