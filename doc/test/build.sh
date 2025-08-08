#!/bin/bash

set -euo pipefail

dir="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
pushd "$dir"


pushd "../.."
file_stem=compiled
adoc_file="doc/test/$file_stem.adoc"
cat <<EOF >"$adoc_file"
= BPMD - Business Process Modeling DSL
:icons: font

EOF
all=(doc/test/*.bpmd)
if [ -n "${BDT:-}" ]; then
    all=("doc/test/${BDT}")
fi
for f in "${all[@]}"; do
    {
        basename=$(basename "$f" .bpmd)
        csv_file="$dir/${basename}.csv"
        correct_csv_file="$dir/${basename}.csv.correct"

        if grep -q '// GENERATE VISIBILITY TABLE' "$f"; then
            cargo run -- -i "$f" -o "${f%.bpmd}.xml" -v "$csv_file" && \
            echo "finished generating ${f%.bpmd}.xml, now generating the png" && \
            doc/node_modules/.bin/bpmn-to-image "${f%.bpmd}.xml":"${f%.bpmd}.png" && \
            : #rm "${f%.bpmd}.xml"
        else
            cargo run -- -i "$f" -o "${f%.bpmd}.xml" && \
            echo "finished generating ${f%.bpmd}.xml, now generating the png" && \
            doc/node_modules/.bin/bpmn-to-image "${f%.bpmd}.xml":"${f%.bpmd}.png" && \
            : #rm "${f%.bpmd}.xml"
        fi

        cat <<EOF >>"$adoc_file"
== $(basename "$f")
image::$(basename "$f" .bpmd).png[width=60%]
EOF

        if grep -q '// GENERATE VISIBILITY TABLE' "$f"; then
            if [[ -f "$correct_csv_file" ]]; then
                if diff -q "$csv_file" "$correct_csv_file" > /dev/null; then
                    echo "✓ Visibility table for $basename matches reference."
                else
                    echo "⚠ Visibility table for $basename differs from reference!"
                    cat <<EOF >>"$adoc_file"
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
                cat <<EOF >>"$adoc_file"
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

            cat <<EOF >>"$adoc_file"
[%header,format=csv]
|===
include::${csv_file}[]
|===
EOF
        fi

        cat <<EOF >>"$adoc_file"
[source]
----
include::$(basename "$f")[]
----
EOF
    } &
done

wait

popd
asciidoctor -o $file_stem.html $file_stem.adoc
