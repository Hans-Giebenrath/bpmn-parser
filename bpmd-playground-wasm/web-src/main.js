const worker = new Worker(
  new URL("./webworker.js", import.meta.url),
  { type: "module" },
);
const worker2 = new Worker(
  new URL("./webworker.js", import.meta.url),
  { type: "module" },
);
import { render_error } from "./editor.js";

const mirror = document.querySelector("#mirror-editor");
const input = document.querySelector("#editor");
const diagram = document.querySelector("#diagram");
const vistab = document.querySelector("#vistab");
const diagnostics_pane = document.querySelector("#diagnostics-pane");
const diagnostics_boxes = document.querySelector("#diag-box-container");

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

worker.addEventListener("message", (event) => {
    if (event.data === "ready") {
        worker1ready = true;
        handleInput();
    }
    if (event.data.type == "Success") {
        diagram.innerHTML = event.data.diagram;
        vistab.innerHTML = event.data.pebpmd_visibility_table_html;
        diagnostics_pane.innerHTML = "";
        diagnostics_boxes.innerHTML = "";
    } else {
        diagnostics_pane.innerHTML = event.data.diagnostics_pane_content;
        diagnostics_boxes.innerHTML = event.data.diagnostic_boxes_html;
        console.log(event);
    }
});

worker2.addEventListener("message", (event) => {
    if (event.data === "ready") {
        worker2ready = true;
        handleInput();
    }
    if (event.data.type == "Success") {
        mirror.innerHTML = event.data.diagram;
        console.log(event.data.diagram);
    } else {
        console.log(event);
        console.log(event.data.error_message);
        console.log(event.data.error_message.replace("\\n", "\n"));
    }
});
