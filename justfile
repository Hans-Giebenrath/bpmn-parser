# Build all examples for the Antora documentation
prepare-doc-site:
    RUSTFLAGS=-Awarnings cargo build
    cd docs-site && run_cargo=false fd -e bpmd --strip-cwd-prefix=always -x ./compile-bpmd.sh

accept-svg image:
    #!/bin/bash
    cd test/
    if [ "{{ image }}" = "all" ]; then
      for f in *.svg; do
        just accept-svg "$f"
      done
      exit 0
    fi
    stem="{{ image }}"
    stem="${stem%.*}"
    cp "$stem.svg" "$stem.correct.svg"
