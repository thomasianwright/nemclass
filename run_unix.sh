#!/bin/sh

cargo b --target release && sudo RUST_BACKTRACE=1 ./target/release/nemclass
