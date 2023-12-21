#!/bin/bash

TARGETS=(aarch64-apple-darwin x86_64-apple-darwin x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu)

for target in "${TARGETS[@]}"; do
  echo "Building $target"
  cross build --release --target $target
done