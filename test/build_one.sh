#!/bin/bash

set -euo pipefail

dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Relative from repo root.
f="$1"
f="$(realpath -e "$f")"

cd "$(git rev-parse --show-toplevel)"
release="${release:-false}"
run() {
    local dir=debug

    if $release; then
        dir=release
    fi
    echo time timeout 3s "${CARGO_TARGET_DIR:-./target}"/$dir/bpmn-parser "$@"
    time timeout 3s "${CARGO_TARGET_DIR:-./target}"/$dir/bpmn-parser "$@" 2>&1
}

basename=$(basename "$f" .bpmd)
tmp_adoc_file="$TMPDIR/$basename.tmp.adoc"
csv_file="$dir/${basename}.csv"
correct_csv_file="$dir/${basename}.csv.correct"
failed=false
vis_table=false

if grep -q '// GENERATE VISIBILITY TABLE' "$f"; then
    run -i "$f" -o "${f%.bpmd}.xml" -v "$csv_file" || failed=true
    vis_table=true
else
    run -i "$f" -o "${f%.bpmd}.xml" || failed=true
fi

cat <<EOF >>"$tmp_adoc_file"
== $(basename "$f")

EOF

if $failed; then
    cat <<EOF >>"$tmp_adoc_file"
WARNING: Build Failure.

EOF
else
    echo "finished generating ${f%.bpmd}.xml, now generating the png"
    test/node_modules/.bin/bpmn-to-image "${f%.bpmd}.xml":"${f%.bpmd}.png"
    rm "${f%.bpmd}.xml"
    cat <<EOF >>"$tmp_adoc_file"
image::$(basename "$f" .bpmd).png[width=60%]

EOF
fi

if $vis_table && ! $failed; then
    if [[ -f "$correct_csv_file" ]]; then
        if diff -q "$csv_file" "$correct_csv_file" >/dev/null; then
            echo "✓ Visibility table for $basename matches reference."
        else
            echo "⚠ Visibility table for $basename differs from reference!"
            cat <<EOF >>"$tmp_adoc_file"
[WARNING]
====
The visibility table differs from the reference: $(basename "$correct_csv_file")

Make sure that:

- The generated output in $(basename "$csv_file") is correct and expected.
- You're intentionally updating the reference.

If so, update it with:

  cp "$(basename "$csv_file")" "$(basename "$correct_csv_file")"

====
EOF
        fi
    else
        echo "⚠ No reference CSV found: $correct_csv_file. Generating warning."
        cat <<EOF >>"$tmp_adoc_file"
[WARNING]
====
No reference visibility table found: $(basename "$correct_csv_file")

Make sure that:

- The generated output in $(basename "$csv_file") is correct and expected.
- You're intentionally updating the reference.

If so, run:

  cp "$(basename "$csv_file")" "$(basename "$correct_csv_file")"

====
EOF
    fi

    cat <<EOF >>"$tmp_adoc_file"
[%header,format=csv]
|===
include::${csv_file}[]
|===
EOF
fi

cat <<EOF >>"$tmp_adoc_file"
[source]
----
include::$(basename "$f")[]
----
EOF
