#!/bin/sh

cargo clean && \
cargo +nightly build -p unpanic && \
cargo +nightly build -p test_executor && \
./target/debug/test_executor
