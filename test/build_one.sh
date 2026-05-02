#!/bin/bash

set -euo pipefail

dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Relative from repo root.
f="$1"
f="$(realpath -e "$f")"

cd "$(git rev-parse --show-toplevel)"
release="${release:-false}"
failed=false
run() {
    local dir=debug

    if $release; then
        dir=release
    fi
    set -x
    echo "OUTPUT FOR $basename" >"$TMPDIR/$basename.output"
    if ! time timeout 3s "${CARGO_TARGET_DIR:-./target}"/$dir/bpmn-parser "$@" 2>&1 | tee -a "$TMPDIR/$basename.output"; then
        failed=true
    fi
    if [[ "$basename" =~ ^ERR ]]; then
        # The ERR* files test that an error actually happens.
        if $failed; then
            failed=false
        else
            failed=true
            failed_filename="$failed_filename (should have shown an error, but was successful)"
        fi
    fi

    if ! $failed; then
        rm "$TMPDIR/$basename.output"
    fi
    set +x
}

basename=$(basename "$f" .bpmd)
failed_filename="error in $basename"
tmp_adoc_file="$TMPDIR/$basename.tmp.adoc"
csv_file="$dir/${basename}.csv"
correct_csv_file="$dir/${basename}.csv.correct"
vis_table=false

if grep -q '// GENERATE VISIBILITY TABLE' "$f"; then
    run -i "$f" -o "${f%.bpmd}.xml" -v "$csv_file"
    vis_table=true
else
    run -i "$f" -o "${f%.bpmd}.xml"
fi

cat <<EOF >>"$tmp_adoc_file"
== $(basename "$f")

EOF

if $failed; then
    echo "$basename" >"$TMPDIR/$failed_filename"

    cat <<EOF >>"$tmp_adoc_file"
WARNING: Build Failure.

EOF
elif ! [[ "$basename" =~ ^ERR ]]; then
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
