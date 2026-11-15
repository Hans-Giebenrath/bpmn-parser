#!/bin/bash

set -euo pipefail

dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

f="$1"
f="$(realpath -e "$f")"

cd "$(git rev-parse --show-toplevel)"
release="${release:-false}"
failed=false
out_format=svg

run() {
    local dir=debug

    if $release; then
        dir=release
    fi
    local no_embed_flag=""
    if [ "${no_embed:-}" = "true" ]; then
        no_embed_flag="--no-svg-embed-font"
    fi
    set -x
    printf "\nOUTPUT FOR %s:\n" "$stem" >"$TMPDIR/$stem.output"
    if ! time timeout 3s "${CARGO_TARGET_DIR:-./target}"/$dir/bpmd $no_embed_flag "$@" 2>&1 | tee -a "$TMPDIR/$stem.output"; then
        failed=true
    fi
    if [[ "$stem" =~ ^ERR ]]; then
        # The ERR* files test that an error actually happens.
        if $failed; then
            failed=false
            while read -r line; do
                if ! grep -F "$line" "$TMPDIR/$stem.output"; then
                    failed=true
                    failed_filename="$failed_filename (output does not contain expected error message: <$line>)"
                    break
                fi
            done < <(<"$f" sed -En 's/(\/\/ ERROR MESSAGE:)\s*(.*)/\2/p')
        else
            failed=true
            failed_filename="$failed_filename (should have shown an error, but was successful)"
        fi
    fi

    if ! $failed; then
        rm "$TMPDIR/$stem.output"
    fi
    set +x
}

stem=$(basename "$f" .bpmd)
failed_filename="error in $stem"
tmp_adoc_file="$TMPDIR/$stem.tmp.adoc"
csv_file="$dir/${stem}.csv"
correct_csv_file="$dir/${stem}.csv.correct"
correct_svg_file="$dir/${stem}.correct.svg"
vis_table=false

if grep -q '// GENERATE VISIBILITY TABLE' "$f"; then
    run -i "$f" -o "${f%.bpmd}.$out_format" -f "$out_format" -v "$csv_file"
    vis_table=true
else
    run -i "$f" -o "${f%.bpmd}.$out_format" -f "$out_format"
fi

cat <<EOF >>"$tmp_adoc_file"
== $(basename "$f")

EOF

if $failed; then
    if grep -q "// TODO" "$f"; then
        failed_filename="$failed_filename (file contains a TODO so likely this issue is known)"
    fi
    echo "$stem" >"$TMPDIR/$failed_filename"

    cat <<EOF >>"$tmp_adoc_file"
WARNING: Build Failure.

EOF
elif ! [[ "$stem" =~ ^ERR ]]; then
    case "$out_format" in
    bpmn)
        echo "finished generating $stem.$out_format, now generating the png"
        test/node_modules/.bin/bpmn-to-image "$dir/$stem.$out_format":"$dir/$stem.png"
        rm "$dir/$stem.$out_format"
        cat <<EOF >>"$tmp_adoc_file"
image::$(basename "$f" .bpmd).png[width=60%]

EOF
        ;;
    svg)
        echo "finished generating $stem.$out_format"
        if ! [ -f "$correct_svg_file" ]; then
            echo "$stem" >"$TMPDIR/missing reference .svg for $stem"
            cat <<EOF >>"$tmp_adoc_file"
WARNING: Reference .svg does not exist.

image::$stem.svg[width=60%]

EOF
        elif cmp --silent "$dir/$stem.svg" "$correct_svg_file"; then
            cat <<EOF >>"$tmp_adoc_file"
image::$stem.svg[width=60%]

EOF
        else
            echo "$stem" >"$TMPDIR/difference in .svg for $stem"
            cat <<EOF >>"$tmp_adoc_file"
WARNING: Reference .svg has different contents.

.New
image::$stem.svg[width=60%]

.Old
image::$stem.correct.svg[width=60%]

EOF

        fi
        ;;

    esac
fi

if $vis_table && ! $failed; then
    if [[ -f "$correct_csv_file" ]]; then
        if diff -q "$csv_file" "$correct_csv_file" >/dev/null; then
            echo "✓ Visibility table for $stem matches reference."
        else
            echo "⚠ Visibility table for $stem differs from reference!"
            echo "$stem" >"$TMPDIR/difference in .csv visibility table for $stem"
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
        echo "$stem" >"$TMPDIR/missing reference .csv visibility table for $stem"
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
