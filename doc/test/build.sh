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
for f in doc/test/t0016.bpmd; do
    {
        if grep -q '// GENERATE VISIBILITY TABLE' "$f"; then
            cargo run -- -i "$f" -o "${f%.bpmd}.xml" -v "${f%.bpmd}.csv" && \
            echo "finished generating ${f%.bpmd}.xml, now generating the png" && \
            doc/node_modules/.bin/bpmn-to-image "${f%.bpmd}.xml":"${f%.bpmd}.png" && \
            : #rm "${f%.bpmd}.xml"
        else
            cargo run -- -i "$f" -o "${f%.bpmd}.xml" && \
            echo "finished generating ${f%.bpmd}.xml, now generating the png" && \
            doc/node_modules/.bin/bpmn-to-image "${f%.bpmd}.xml":"${f%.bpmd}.png" && \
            : #rm "${f%.bpmd}.xml"
        fi
    } &
    if grep -q '// GENERATE VISIBILITY TABLE' "$f"; then
        cat <<EOF >>"$adoc_file"
== $(basename "$f")
image::$(basename "$f" .bpmd).png[width=60%]
[%header,format=csv]
|===
include::$(basename "$f" .bpmd).csv[]
|===
[source]
----
include::$(basename "$f")[]
----
EOF
    else
        cat <<EOF >>"$adoc_file"
== $(basename "$f")
image::$(basename "$f" .bpmd).png[width=60%]
[source]
----
include::$(basename "$f")[]
----
EOF
    fi
done

wait

popd
asciidoctor -o $file_stem.html $file_stem.adoc
