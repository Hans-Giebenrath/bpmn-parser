const worker = new Worker(
  new URL("./webworker.js", import.meta.url),
  { type: "module" },
);
const worker2 = new Worker(
  new URL("./webworker.js", import.meta.url),
  { type: "module" },
);

import { encode, decode, download_file } from "./util.js";

const mirror = document.querySelector("#mirror-editor");
const input = document.querySelector("#editor");
const diagram = document.querySelector("#diagram");
const vistab = document.querySelector("#vistab");
const diagnostics_pane = document.querySelector("#diagnostics-pane");
const diagnostics_boxes = document.querySelector("#diag-box-container");
const example_select = document.querySelector("#example-selection");
const download_select = document.querySelector("#download-selection");
const cheat_sheet_small = document.querySelector("#cheat-sheet-small");
let editor_content_modified = false;

let worker1ready = false;
let worker2ready = false;
const handleInput = () => {
    editor_content_modified = true;
    if (worker1ready && worker2ready) {
        console.log("Here!", input.value);
        worker.postMessage({ text: input.value, format: "SvgNoEmbed" });
        worker2.postMessage({ text: input.value, format: "HighlightedInnerHtml" });
    }
};

input.addEventListener("input", handleInput);

let download_resolver = {
    // `null` or `(data) => void`
    on_success: null,
    // `null` or `() => void`
    on_error: null,
};

function resolve_download_resolver(data_or_null)  {
    let was_resolved = false;
    try {
        if (data_or_null !== null) {
            if (download_resolver.on_success) {
                was_resolved = true;
                download_resolver.on_success(data_or_null);
            }
        } else {
            if (download_resolver.on_error) {
                was_resolved = true;
                download_resolver.on_error();
            }
        }
    } catch (e) {
        console.error("Download failed", e);
    }
    if (was_resolved) {
        download_resolver.on_success = null;
        download_resolver.on_error = null;
        download_select.disabled = false;
    }
    return was_resolved;
}

worker.addEventListener("message", (event) => {
    if (event.data === "ready") {
        worker1ready = true;
        handleInput();
        example_select.disabled = false;
        download_select.disabled = false;
        return;
    }
    console.log("diagram rendering", event.data);
    if (event.data.type == "Success") {
        if (resolve_download_resolver(event.data)) {
            return;
        } else {
            diagram.innerHTML = event.data.diagram;
            vistab.innerHTML = event.data.pebpmd_visibility_table_html;
        }
    } else {
        if (resolve_download_resolver(null)) {
            return;
        }
    }

    diagnostics_pane.innerHTML = event.data.diagnostics_pane_content;
    diagnostics_boxes.innerHTML = event.data.diagnostic_boxes_html;
});

worker2.addEventListener("message", (event) => {
    if (event.data === "ready") {
        worker2ready = true;
        handleInput();
        return;
    }
    console.log("syntax highlighting", event.data);
    if (event.data.type == "Success") {
        mirror.innerHTML = event.data.diagram;
    } else {
        // Well this should never happen?
        mirror.textContent = input.value;
    }
});

input.addEventListener("scroll", () => {
    front_mirror.style.transform =
        `translate(${-input.scrollLeft}px, ${-input.scrollTop}px)`;
});

example_select.addEventListener("change", async (event) => {
    const example = event.target.value;

    if (!example) return;

    if (!worker1ready) {
        alert("Whoops? worker1 is not ready, yet? The select should be disabled.");
        return;
    }

    const idx = event.target.selectedIndex;
    await load_example(example);
    event.target.selectedIndex = idx;
    event.target.focus();
});
example_select.addEventListener('blur', (event) => {
  event.target.selectedIndex = 0;
});
example_select.selectedIndex = 0;

download_select.addEventListener("change", async (event) => {
    const action = event.target.value;

    if (!action) return;

    // Reset to the initial option
    event.target.selectedIndex = 0;

    if (!worker1ready) {
        alert("Whoops? worker1 is not ready, yet? The select should be disabled.");
        return;
    }
    event.target.disabled = true;

    switch (action) {
        case "svg-embed":
            download_resolver.on_success = (data) => {
                download_file(data.diagram, "diagram.svg", "image/svg+xml");
            };
            download_resolver.on_error = () => {
                alert("Failed to create the SVG, ensure that you have no errors in your diagram.");
            };
            worker.postMessage({ text: input.value, format: "SvgEmbed" });
            break;
        case "svg-no-embed":
            download_resolver.on_success = (data) => {
                download_file(data.diagram, "diagram.svg", "image/svg+xml");
            };
            download_resolver.on_error = () => {
                alert("Failed to create the SVG, ensure that you have no errors in your diagram.");
            };
            worker.postMessage({ text: input.value, format: "SvgNoEmbed" });
            break;
        case "bpmn-camunda":
            download_resolver.on_success = (data) => {
                download_file(data.diagram, "diagram.bpmn", "application/xml");
            };
            download_resolver.on_error = () => {
                alert("Failed to create the BPMN file, ensure that you have no errors in your diagram.");
            };
            worker.postMessage({ text: input.value, format: "Bpmn" });
            break;
        case "vistab-csv":
            download_resolver.on_success = (data) => {
                download_file(data.diagram, "visibility-table.csv", "text/csv");
            };
            download_resolver.on_error = () => {
                alert("Failed to create the CSV, ensure that you have no errors in your diagram.");
            };
            worker.postMessage({ text: input.value, format: "VisTabCsv" });
            break;
        case "diag-txt":
            download_file(input.value, "diagram.txt", "text/plain");
            event.target.disabled = false;
            break;
        case "link":
            event.target.disabled = false;
            const url = new URL(window.location.href);
            url.searchParams.set("d", encode(input.value));
            await navigator.clipboard.writeText(url.toString());
            break;
    }

});

async function load_example(value) {
    if (editor_content_modified) {
        const confirmed = confirm('Do you want to override your diagram code with an example?');
        if (!confirmed) {
            return;
        }
    }
    try {
        const response = await fetch(`examples/${encodeURIComponent(value)}.bpmd`);

        if (!response.ok) {
            throw new Error(`HTTP ${response.status}`);
        }

        input.value = await response.text();

        // Trigger diagram rendering.
        input.dispatchEvent(new Event('input', { bubbles: true }));

        // Set it after the event dispatch, as that one sets the flag to `true`.
        editor_content_modified = false;
    } catch (error) {
        console.error('Failed to load example:', error);
    }
}

{
    try {
        if (input.value == "") {
            const example = new URLSearchParams(location.search).get('example');
            const diag = new URLSearchParams(location.search).get('d');
            if (example !== null) {
                await load_example(example);
            } else if (diag !== null) {
                input.value = decode(diag);
                // Trigger diagram rendering.
                input.dispatchEvent(new Event('input', { bubbles: true }));
            }
        }
    } catch (e) {
        console.error('Failed to initialize website from URL parameter:', error);
    }

    // Remove the `?...#...` part of the URL, as we use that one to load examples and reloading
    // then reloads the example and not the modification.
    history.replaceState(null, '', location.pathname);
}

cheat_sheet_small.addEventListener('click', async (event) => {
    const target = event.target.closest('a[href]');

    if (!target || !cheat_sheet_small.contains(target)) return;

    const url = new URL(target.href);
    // Only handle links with an example parameter
    const value = url.searchParams.get('example');
    if (value === null) return;

    // Only intercept ordinary left-clicks
    if (
        event.button !== 0 ||
        event.ctrlKey ||
        event.metaKey ||
        event.shiftKey ||
        event.altKey
    ) return;

    event.preventDefault();

    await load_example(value);
});
