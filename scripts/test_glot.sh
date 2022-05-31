#!/bin/bash

BASE_URL="$1"
ACCESS_TOKEN="$2"


run() {
    DOCKER_IMAGE=$1
    PAYLOAD_PATH=$2

    JSON="{ \"image\": \"${DOCKER_IMAGE}\", \"payload\": $(cat "$PAYLOAD_PATH") }"

    RUN_RESULT=$(curl \
        -X POST \
        -H "Content-type: application/json" \
        -H "X-Access-Token: $ACCESS_TOKEN" \
        --silent \
        --data "$JSON" \
        --url "${BASE_URL}/run")

    echo "${DOCKER_IMAGE} [${PAYLOAD_PATH}]: ${RUN_RESULT}"
}

run "glot/assembly:latest" "payload/assembly.json"
run "glot/ats:latest" "payload/ats.json"
run "glot/bash:latest" "payload/bash.json"
run "glot/clang:latest" "payload/c.json"
run "glot/clang:latest" "payload/cpp.json"
run "glot/clojure:latest" "payload/clojure.json"
run "glot/cobol:latest" "payload/cobol.json"
run "glot/coffeescript:latest" "payload/coffeescript.json"
run "glot/crystal:latest" "payload/crystal.json"
run "glot/dlang:latest" "payload/dlang.json"
run "glot/elixir:latest" "payload/elixir.json"
run "glot/elm:latest" "payload/elm.json"
run "glot/erlang:latest" "payload/erlang.json"
run "glot/golang:latest" "payload/golang.json"
run "glot/groovy:latest" "payload/groovy.json"
run "glot/haskell:latest" "payload/haskell.json"
run "glot/idris:latest" "payload/idris.json"
run "glot/java:latest" "payload/java.json"
run "glot/javascript:latest" "payload/javascript.json"
run "glot/julia:latest" "payload/julia.json"
run "glot/kotlin:latest" "payload/kotlin.json"
run "glot/lua:latest" "payload/lua.json"
run "glot/mercury:latest" "payload/mercury.json"
run "glot/csharp:latest" "payload/csharp.json"
run "glot/fsharp:latest" "payload/fsharp.json"
run "glot/nim:latest" "payload/nim.json"
run "glot/ocaml:latest" "payload/ocaml.json"
run "glot/perl:latest" "payload/perl.json"
run "glot/php:latest" "payload/php.json"
run "glot/python:latest" "payload/python.json"
run "glot/raku:latest" "payload/raku.json"
run "glot/ruby:latest" "payload/ruby.json"
run "glot/rust:latest" "payload/rust.json"
run "glot/scala:latest" "payload/scala.json"
run "glot/swift:latest" "payload/swift.json"
run "glot/typescript:latest" "payload/typescript.json"
