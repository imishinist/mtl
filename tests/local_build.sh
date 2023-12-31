#!/bin/bash

. $(dirname $0)/common.inc

cd $(setup_new case1)

# case1
hash="99f9d6592fc5edec"
$MTL local build | grep -Eq "\s${hash}$"
cat .mtl/HEAD | grep -Eq "^${hash}$"

# only listed file:  "-i" option
hash="562bd68f2c83dfe2"
$MTL local build -i <(echo 'README') | grep -Eq "\s${hash}$"
cat .mtl/HEAD | grep -Eq "^${hash}$"

echo 'README' | $MTL local build -i - | grep -Eq "\s${hash}$"
cat .mtl/HEAD | grep -Eq "^${hash}$"

hash="ae65013be93d648e"
$MTL local build -i <(echo '.ignore') | grep -Eq "\s${hash}$"
cat .mtl/HEAD | grep -Eq "^${hash}$"


# hidden file:  "--hidden" option
hash="6b1d722afb0c117d"
$MTL local build --hidden | grep -Eq "\s${hash}$"
cat .mtl/HEAD | grep -Eq "^${hash}$"