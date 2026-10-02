# Build all examples for the Antora documentation
prepare-doc-site:
    RUSTFLAGS=-Awarnings cargo build
    cd docs-site && run_cargo=false fd -e bpmd --strip-cwd-prefix=always -x ./compile-bpmd.sh

accept-svg image:
    #!/bin/bash
    cd test/
    if [ "{{ image }}" = "all" ]; then
      for f in *.svg; do
        if [[ "$f" =~ \.correct\. ]]; then continue; fi
        just accept-svg "$f" &
      done
      wait
      exit 0
    fi
    stem="{{ image }}"
    stem="${stem%.*}"
    cp "$stem.svg" "$stem.correct.svg"

build-playground out_dir:
    #!/usr/bin/bash
    set -x
    cd bpmd-playground-wasm
    binary_file="{{ out_dir }}"/bpmd_playground_wasm.js
    if ! [ -f "$binary_file" ] || find . -name '*.rs' -newer "$binary_file" -print -quit | grep -q .; then
        # Sources are newer, rebuild.
        ~/.cargo/bin/wasm-pack build --release --target web --out-dir "{{ out_dir }}"
    fi
    cp -r web-src/. "{{ out_dir }}/"

_test-playground test_dir:
    #!/usr/bin/bash
    set -euo pipefail
    just build-playground "{{ test_dir }}"
    cd "{{ test_dir }}/"
    tput bel
    python3 -m http.server

test-playground:
    #!/usr/bin/bash
    test_dir="$(mktemp -d --tmpdir bpmd-playground.XXXX)"
    cleanup() {
        rm -r "$test_dir"
    }
    trap "cleanup" EXIT
    watchexec --on-busy-update restart --watch bpmd-playground-wasm/src --watch bpmd-playground-wasm/web-src just _test-playground "$test_dir"
