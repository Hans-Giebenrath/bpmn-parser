import init, { compile_bpmd, init_bpmd } from "./bpmd_playground_wasm.js";

await init();
init_bpmd();

self.addEventListener("message", (event) => {
    try {
        const result = compile_bpmd(event.data);
        self.postMessage(result)
    } catch (e) {
        console.log(e);
    }
});

self.postMessage("ready");
