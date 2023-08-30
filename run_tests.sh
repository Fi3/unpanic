#!/bin/sh

cargo clean && \
cargo build -p unpanic && \
cargo build -p test_executor && \
./target/debug/test_executor
