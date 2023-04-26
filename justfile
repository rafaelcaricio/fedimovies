#!/usr/bin/env -S just --justfile

build-release:
  cargo build --release --target x86_64-unknown-linux-gnu
  cp target/x86_64-unknown-linux-gnu/release/fedimovies build/fedimovies
  cp target/x86_64-unknown-linux-gnu/release/fedimoviesctl build/fedimoviesctl

deploy: build-release
  fly deploy
