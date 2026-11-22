#!/bin/bash -eu
# Build every cargo-fuzz target and ship its seed corpus (ClusterFuzzLite / OSS-Fuzz).
cd "$SRC/ftar-core"
cargo fuzz build -O
for target in $(cargo fuzz list); do
  cp "fuzz/target/x86_64-unknown-linux-gnu/release/$target" "$OUT/"
  if [ -d "fuzz/corpus/$target" ]; then
    (cd "fuzz/corpus/$target" && zip -q -r "$OUT/${target}_seed_corpus.zip" .)
  fi
done
