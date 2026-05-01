#!/bin/bash

set -euo pipefail

dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
pushd "$dir"

# The TMPDIR stores intermediate .adoc and .csv fragments.
# All the BPMD are compiled in parallel. To ensure that the resulting document
# contains the fragments in the correct order (glob order .. maybe improve),
# the fragments are first written in parallel into smaller files, and then
# in the end all those files are combined sequentially.
TMPDIR=$(mktemp -d -t 'bpmn-parser-test_build.sh.XXXXXXXX')
export TMPDIR
export RUSTFLAGS="${RUSTFLAGS:--Awarnings}"
export RUST_BACKTRACE="${RUST_BACKTRACE:-1}"

print_in_red() {
    printf "\033[31m%s\033[0m\n" "$*"
}

cleanup() {
    rm -rf "$TMPDIR"
}
trap cleanup EXIT

if [ "$#" -eq 0 ]; then
    all=(*.bpmd)
else
    all=("$@")
fi
release="${release:-false}"
if [ "$release" = "true" ]; then
    (cd .. && cargo build --release)
else
    (cd .. && cargo build)
fi

file_stem=compiled
adoc_file="$file_stem.adoc"

parallelism=2
echo "${all[@]}"
for f in "${all[@]}"; do
    if ((parallelism == 0)); then
        wait -n || true
    else
        ((--parallelism)) || true
    fi
    ./build_one.sh "$f" &
done
wait

cat <<EOF >"$adoc_file"
= BPMD - Business Process Modeling DSL
:icons: font

EOF

for f in "${all[@]}"; do
    basename=$(basename "$f" .bpmd)
    tmp_adoc_file="$TMPDIR/$basename.tmp.adoc"
    cat "$tmp_adoc_file" >>"$adoc_file"
done

popd
asciidoctor -o $file_stem.html $file_stem.adoc

# What a dirty hack! I use file names as error messages to print to the user.
# Does the job for this little tool.
cd "$TMPDIR"

shopt -s nullglob
some_failed=false
for f in error*; do
    print_in_red "$f"
    some_failed=true
done

if $some_failed; then
    cat -- *.output >"$dir/failed_runs_output"
    print_in_red "Error messages can be reviewed in file: ./failed_runs_output."
fi
