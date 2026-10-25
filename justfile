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
    set -xeuo pipefail
    outdir="$(realpath -e "{{ out_dir }}")"
    cd bpmd-playground-wasm
    binary_file="$outdir/bpmd_playground_wasm.js"
    if ! [ -f "$binary_file" ] || find .. -name '*.rs' -newer "$binary_file" -print -quit | grep -q .; then
        # Sources are newer, rebuild.
        RUSTFLAGS="-C panic=unwind -C target-feature=+exception-handling" \
          cargo +nightly build \
            --release \
            --target wasm32-unknown-unknown \
            -Z build-std=std,panic_unwind
        ~/.cargo/bin/wasm-bindgen \
          --target web \
          --out-dir "$outdir" \
          "${CARGO_TARGET_DIR:-target}/wasm32-unknown-unknown/release/bpmd_playground_wasm.wasm"
        tput bel
    fi
    mkdir -p "$outdir/fonts"
    cp ../inter-font/Inter-{Italic,Regular,SemiBold}.ttf "$outdir/fonts"
    cp -r web-src/. "$outdir/"

_test-playground test_dir:
    #!/usr/bin/bash
    set -euo pipefail
    just build-playground "{{ test_dir }}"
    cd "{{ test_dir }}/"
    python3 -m http.server

test-playground:
    #!/usr/bin/bash
    test_dir="$(mktemp -d --tmpdir bpmd-playground.XXXX)"
    cleanup() {
        rm -r "$test_dir"
    }
    trap "cleanup" EXIT
    watchexec --on-busy-update restart --exts html,css,js,rs,toml just _test-playground "$test_dir"
