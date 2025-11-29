#!/bin/bash

set -euo pipefail

dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
pushd "$dir"

# The TMPDIR stores intermediate .adoc and .csv fragments.
# All the BPMD are compiled in parallel. To ensure that the resulting document
# contains the fragments in the correct order (glob order .. maybe improve),
# the fragments are first written in parallel into smaller files, and then
# in the end all those files are combined sequentially.
TMPDIR=$(mktemp -d -t 'bpmn-parser-doc_test_build.sh.XXXXXXXX')

cleanup() {
    rm -rf "$TMPDIR"
}
trap cleanup EXIT

pushd "../.."
release="${release:-false}"
if [ "$release" = "true" ]; then
    cargo build --release
else
    cargo build
fi
run() {
    if $release; then
        time "${CARGO_TARGET_DIR:-./target}"/release/bpmn-parser "$@"
    else
        time "${CARGO_TARGET_DIR:-./target}"/debug/bpmn-parser "$@"
    fi
}
file_stem=compiled
adoc_file="doc/test/$file_stem.adoc"

# Environment variable BDT can be used to select just one file for compilation.
# Usually one wants to test all files at once, or one composes a new file or
# debugs on one file. Hence no `many` selection at the moment, either one or all.
all=(doc/test/*.bpmd)
if [ -n "${BDT:-}" ]; then
    all=("doc/test/${BDT}")
fi
for f in "${all[@]}"; do
    {
        basename=$(basename "$f" .bpmd)
        tmp_adoc_file="$TMPDIR/$basename.tmp.adoc"
        csv_file="$dir/${basename}.csv"
        correct_csv_file="$dir/${basename}.csv.correct"
        failed=false

        if grep -q '// GENERATE VISIBILITY TABLE' "$f"; then
            if run -i "$f" -o "${f%.bpmd}.xml" -v "$csv_file"; then
                echo "finished generating ${f%.bpmd}.xml, now generating the png" &&
                    doc/node_modules/.bin/bpmn-to-image "${f%.bpmd}.xml":"${f%.bpmd}.png" &&
                    rm "${f%.bpmd}.xml"
            else
                failed=true
            fi
        else
            if run -i "$f" -o "${f%.bpmd}.xml"; then
                echo "finished generating ${f%.bpmd}.xml, now generating the png" &&
                    doc/node_modules/.bin/bpmn-to-image "${f%.bpmd}.xml":"${f%.bpmd}.png" &&
                    cp "${f%.bpmd}.xml" /tmp/ &&
                    rm "${f%.bpmd}.xml"
            else
                failed=true
            fi
        fi

        cat <<EOF >>"$tmp_adoc_file"
== $(basename "$f")
image::$(basename "$f" .bpmd).png[width=60%]
EOF

        if $failed; then
            cat <<EOF >>"$tmp_adoc_file"

WARNING: Build Failure.

EOF

        fi

        if grep -q '// GENERATE VISIBILITY TABLE' "$f"; then
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
    }
done

#wait

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
