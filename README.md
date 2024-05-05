# mtl

[![Rust](https://github.com/imishinist/mtl/actions/workflows/rust.yml/badge.svg)](https://github.com/imishinist/mtl/actions/workflows/rust.yml)

Tools for recursively computing and indexing directory hashes.

It is based on git's tree object structure.

![Demo](assets/demo.gif)

## How to use

build index

```bash
$ mtl local build
Written HEAD: 80bef6537f9c4f9d
```

print tree

```bash
$ mtl print-tree --type tree --max-depth 1
tree 80bef6537f9c4f9d   .
tree 757dfd8c7ed0c1b6   benches/
tree d68f7fd0eec160a2   src/
tree 47d8072d2b99a537   tools/
```

Please read the atmosphere from help for more information.


## How to install

```bash
$ cargo install --git https://github.com/imishinist/mtl
```

## Performance check

```bash
$ ./tools/compare-performance.sh <dir> <revision> <revision> [hyperfine options] 
```

example

```bash
$ mtl tool generate /tmp/bench10000 10000 -p 1,2
$ ./tools/compare-performance.sh /tmp/bench10000 HEAD HEAD^ '--warmup 3'
```
