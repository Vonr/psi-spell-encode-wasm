#!/bin/sh

build() {
    wasm-pack build --release -t "$1" --out-name psi-spell-encode-wasm --features wasm || exit 1
    jq '.name = "psi-spell-encode-wasm"' pkg/package.json >pkg/package.json.temp || exit 1
    mv pkg/package.json.temp pkg/package.json || exit 1
}

case "$1" in
'web')
    printf 'Building wasm for web\n'
    build web
    ;;
'node')
    printf 'Building wasm for node\n'
    build nodejs
    ;;
'repl')
    printf 'Building wasm for node\n'
    build nodejs
    node -e 'var pkg = require("./pkg"); process.stdout.write("\rPackage is available as '\''pkg'\''.\n> ");' -i
    ;;
*)
    printf 'Unrecognized command.\nUsage: %s [web|node]\n' "$0"
    exit 1
    ;;
esac
