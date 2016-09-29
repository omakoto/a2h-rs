#!/bin/bash

. mutil.sh

RUST_BACKTRACE=1 RUST_LOG='a2h=debug' cargo test -- --nocapture
