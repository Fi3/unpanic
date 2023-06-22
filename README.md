clear && cargo clean && RUST_BACKTRACE=1 RUSTC_WRAPPER=/home/user/temp/no-pan/target/debug/unpanic TARGET_CRATE=prova cargo +nightly build
clear && cargo clean && RUSTC_WRAPPER=/home/user/temp/no-pan/target/debug/unpanic TARGET_CRATE=prova cargo +nightly build

clear && cargo clean && cargo +nightly build -p unpanic && RUSTC_WRAPPER=./target/debug/unpanic TARGET_CRATE=test1_bin cargo +nightly build -p test1_bin


# Phases

# How panic are catched

# Std lib

...

