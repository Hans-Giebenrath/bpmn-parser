# Build all examples for the Antora documentation
prepare-doc-site:
    RUSTFLAGS=-Awarnings cargo build
    cd docs-site && run_cargo=false fd -e bpmd --strip-cwd-prefix=always -x ./compile-bpmd.sh
