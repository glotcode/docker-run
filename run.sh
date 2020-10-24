#!/bin/bash

export SERVER_LISTEN_ADDR="127.0.0.1"
export SERVER_LISTEN_PORT="8088"
export SERVER_LISTEN_THREADS="10"
export UNIX_SOCKET_PATH="/Users/pii/Library/Containers/com.docker.docker/Data/docker.raw.sock"
export UNIX_SOCKET_READ_TIMEOUT="3"
export UNIX_SOCKET_WRITE_TIMEOUT="3"
export RUST_LOG=info

cargo run
