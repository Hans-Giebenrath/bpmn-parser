const worker = new Worker(
  new URL("./webworker.js", import.meta.url),
  { type: "module" },
);
const worker2 = new Worker(
  new URL("./webworker.js", import.meta.url),
  { type: "module" },
);

const mirror = document.querySelector("#mirror-editor");
const input = document.querySelector("#editor");
const diagram = document.querySelector("#diagram");
const vistab = document.querySelector("#vistab");
const diagnostics_pane = document.querySelector("#diagnostics-pane");
const diagnostics_boxes = document.querySelector("#diag-box-container");
const download_select = document.querySelector("#download-selection");

let worker1ready = false;
let worker2ready = false;
const handleInput = () => {
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

function downloadFile(content, filename, mimeType) {
    const blob = new Blob([content], { type: mimeType });
    const url = URL.createObjectURL(blob);

    const a = document.createElement("a");
    a.href = url;
    a.download = filename;
    a.click();

    URL.revokeObjectURL(url);
}

download_select.addEventListener("change", (event) => {
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
                downloadFile(data.diagram, "diagram.svg", "image/svg+xml");
            };
            download_resolver.on_error = () => {
                alert("Failed to create the SVG, ensure that you have no errors in your diagram.");
            };
            worker.postMessage({ text: input.value, format: "SvgEmbed" });
            break;
        case "svg-no-embed":
            download_resolver.on_success = (data) => {
                downloadFile(data.diagram, "diagram.svg", "image/svg+xml");
            };
            download_resolver.on_error = () => {
                alert("Failed to create the SVG, ensure that you have no errors in your diagram.");
            };
            worker.postMessage({ text: input.value, format: "SvgNoEmbed" });
            break;
        case "bpmn-camunda":
            download_resolver.on_success = (data) => {
                downloadFile(data.diagram, "diagram.bpmn", "application/xml");
            };
            download_resolver.on_error = () => {
                alert("Failed to create the BPMN file, ensure that you have no errors in your diagram.");
            };
            worker.postMessage({ text: input.value, format: "Bpmn" });
            break;
        case "vistab-csv":
            download_resolver.on_success = (data) => {
                downloadFile(data.diagram, "visibility-table.csv", "text/csv");
            };
            download_resolver.on_error = () => {
                alert("Failed to create the CSV, ensure that you have no errors in your diagram.");
            };
            worker.postMessage({ text: input.value, format: "VisTabCsv" });
            break;
        case "diag-txt":
            downloadFile(input.value, "diagram.txt", "text/plain");
            event.target.disabled = false;
            break;
    }

});
