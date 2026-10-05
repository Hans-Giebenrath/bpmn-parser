const editor = document.querySelector("#editor");
const mirror = document.querySelector("#mirror-editor");

editor.addEventListener("scroll", () => {
  mirror.style.transform =
    `translate(${-textarea.scrollLeft}px, ${-textarea.scrollTop}px)`;
});

editor.addEventListener("input", () => {
    mirror.textContent = editor.value;
});

export function render_error() {}
