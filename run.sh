#!/bin/bash

export SERVER_LISTEN_ADDR="127.0.0.1"
export SERVER_LISTEN_PORT="8088"
export SERVER_WORKER_THREADS="10"
export UNIX_SOCKET_PATH="/Users/pii/Library/Containers/com.docker.docker/Data/docker.raw.sock"
export UNIX_SOCKET_READ_TIMEOUT="3"
export UNIX_SOCKET_WRITE_TIMEOUT="3"
export RUN_MAX_EXECUTION_TIME="10"
export RUN_MAX_OUTPUT_SIZE="100000"
export RUST_LOG=debug

cargo run
