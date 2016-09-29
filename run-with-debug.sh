#!/bin/bash

. mutil.sh

RUST_LOG=a2h=debug cargo run -- "$@"
