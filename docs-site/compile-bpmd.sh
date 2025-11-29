#!/usr/bin/bash

set -euo pipefail

die() {
    >&2 echo "$*"
    exit 1
}

if [ $# -ne 1 ]; then
    die "Provide one argument, path to .bpmd file."
fi

bpmd_file="$1"
if [[ "$bpmd_file" != modules/ROOT/examples/*.bpmd ]]; then
    die "The file you provided is not of the form modules/ROOT/examples/*.bpmd"
fi

dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
images_dir="$dir/modules/ROOT/images/generated"
csv_dir="$dir/modules/ROOT/examples/generated"
mkdir -p "$images_dir" "$csv_dir"
xml_file="$dir/${bpmd_file%.bpmd}.xml"
csv_file="$csv_dir${bpmd_file#modules/ROOT/examples}"
csv_file="${csv_file%.bpmd}.csv"
png_file="$images_dir${bpmd_file#modules/ROOT/examples}"
png_file="${png_file%.bpmd}.png"
bpmd_file="$dir/$bpmd_file"
cd "$dir"

if [ "${force_build:-}" != "true" ] && [ -e "$png_file" ] && [ -e "$csv_file" ] && [ "$png_file" -nt "$bpmd_file" ] && [ "$csv_file" -nt "$bpmd_file" ]; then
    echo "Skipping build for $1, since the png and csv are newer"
    exit 0
fi

# The TMPDIR stores intermediate .adoc and .csv fragments.
TMPDIR=$(mktemp -d -t 'bpmn-parser-compile-bpmd.sh.XXXXXXXX')

cleanup() {
    rm -rf "$TMPDIR"
}
trap cleanup EXIT

cd "$(git rev-parse --show-toplevel)"
if [ "${release:-}" = "true" ]; then
    release_dir="release"
    if [ "${run_cargo:-true}" = "true" ]; then
        cargo build --release
    fi
else
    release_dir="debug"
    if [ "${run_cargo:-true}" = "true" ]; then
        cargo build
    fi
fi
run() {
    time "${CARGO_TARGET_DIR:-./target}"/${release_dir}/bpmn-parser "$@"
}

echo "generating "
run -i "$bpmd_file" -o "$xml_file" -v "$csv_file"
echo "finished generating $xml_file, now generating the png"
doc/node_modules/.bin/bpmn-to-image "$xml_file":"$png_file"
rm "$xml_file"
