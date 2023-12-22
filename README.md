# mtl

Tools for recursively computing and indexing directory hashes.

It is based on git's tree object structure.

## How to use

build index

```bash
$ mtl local build
Written HEAD: 4510a532ba4f0bef41590dafd234d5ac
```

print tree

```bash
$ mtl print-tree --type tree --max-depth 1
tree 4510a532ba4f0bef41590dafd234d5ac   <root>
tree 13019075d0da958d41d3715c437a6725   benches/
tree 32a9b093ad70021c1af3a9f76a54dadd   src/
tree baf3a3fec2c204a4c266ddc05ff37724   valgrind/
```

Please read the atmosphere from help for more information.


## How to install

```bash
$ cargo install --git https://github.com/imishinist/mtl
```

