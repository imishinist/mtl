#!/bin/bash

DROP_LIST=$(mktemp)

export MTL="./mtl"
export RUST_BACKTRACE=1

export GREEN=$(tput setaf 2)
export RED=$(tput setaf 1)
export RESET=$(tput sgr0)

function echo_green() {
  echo "${GREEN}$1${RESET}"
}

function echo_red() {
  echo "${RED}$1${RESET}"
}

function setup_new() {
  case=$1

  current_dir=$(pwd)
  dir=$(mktemp -d)
  echo $dir >> $DROP_LIST

  cp -r "$current_dir/tests/boilerplate/$case/" $dir
  rm -rf $dir/.mtl

  cd $dir
  ln -s $current_dir/target/debug/mtl mtl
  echo $dir
}

on_error() {
  code=$?
  echo "command failed: $1: $BASH_COMMAND"
  cat $DROP_LIST
  trap - EXIT
  exit $code
}

on_exit() {
  while read -r dir; do
    rm -rf $dir
  done < $DROP_LIST
  echo_green OK
  exit 0
}

trap 'on_error $LINENO' ERR
trap 'on_exit' EXIT

testname=$(basename "$0" .sh)
echo -n "Testing $testname ... "
