/* The search overlay: opening, closing, and keyboard handling. The index
   reader is a separate lazily loaded module with its own budget (§12.2), so
   this file stays in the base bundle and that one does not. */
(function () {
  var dialog = document.querySelector("[data-ly-search]");
  var trigger = document.querySelector("[data-ly-search-trigger]");
  if (!dialog || !trigger) return;

  var input = dialog.querySelector(".ly-search-input");
  var lastFocus = null;
  var loading = null;

  function load() {
    var source = dialog.getAttribute("data-ly-search-module");
    if (!source || loading) return loading;
    loading = import(source)["catch"](function () {
      loading = null;
    });
    return loading;
  }

  function open() {
    lastFocus = document.activeElement;
    dialog.hidden = false;
    window.requestAnimationFrame(function () {
      dialog.setAttribute("data-ly-open", "true");
    });
    if (input) input.focus();
    load();
    window.liyasa.emit("search:open", {});
  }

  function close() {
    dialog.removeAttribute("data-ly-open");
    window.setTimeout(function () {
      dialog.hidden = true;
    }, 200);
    if (lastFocus) lastFocus.focus();
    window.liyasa.emit("search:close", {});
  }

  trigger.addEventListener("click", open);
  trigger.hidden = false;

  dialog.addEventListener("click", function (event) {
    if (event.target === dialog) close();
  });

  document.addEventListener("keydown", function (event) {
    var typing = /^(input|textarea|select)$/i.test(event.target.tagName || "");
    if ((event.key === "k" && (event.metaKey || event.ctrlKey)) || (event.key === "/" && !typing)) {
      event.preventDefault();
      open();
      return;
    }
    if (event.key === "Escape" && !dialog.hidden) close();
  });

  dialog.addEventListener("keydown", function (event) {
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    var results = Array.prototype.slice.call(dialog.querySelectorAll(".ly-search-result"));
    if (results.length === 0) return;
    event.preventDefault();
    var at = results.findIndex(function (result) {
      return result.getAttribute("aria-selected") === "true";
    });
    var next = event.key === "ArrowDown" ? at + 1 : at - 1;
    if (next < 0) next = results.length - 1;
    if (next >= results.length) next = 0;
    results.forEach(function (result, index) {
      result.setAttribute("aria-selected", String(index === next));
    });
    results[next].scrollIntoView({ block: "nearest" });
  });
})();
