const worker = new Worker(
  new URL("./webworker.js", import.meta.url),
  { type: "module" },
);

const input = document.querySelector("#editor");
const diagram = document.querySelector("#diagram");
const vistab = document.querySelector("#vistab");
const diagnostics = document.querySelector("#diagnostics");

input.addEventListener("input", () => {
  worker.postMessage({ text: input.value, format: "SvgEmbed" });
});

worker.addEventListener("message", (event) => {
    if (event.data.type == "Success") {
        diagram.innerHTML = event.data.diagram;
        vistab.innerHTML = event.data.pebpmd_visibility_table_html;
        diagnostics.textContent = "";
    } else {
        diagnostics.textContent = event.data.error_message.replace("\n", "\n\r");
        console.log(event);
        console.log(event.data.error_message);
        console.log(event.data.error_message.replace("\\n", "\n"));
    }
});
