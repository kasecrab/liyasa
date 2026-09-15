/* Lazily loaded: the assistant panel's shell. Budgeted apart from the base
   bundle (THM-31) and only fetched when the reader asks for it. */
(function () {
  var panel = document.querySelector("[data-ly-assistant]");
  var trigger = document.querySelector("[data-ly-assistant-trigger]");
  if (!panel || !trigger) return;

  function open() {
    panel.hidden = false;
    var input = panel.querySelector("input, textarea");
    if (input) input.focus();
    window.liyasa.emit("assistant:open", {});
  }

  function close() {
    panel.hidden = true;
    trigger.focus();
    window.liyasa.emit("assistant:close", {});
  }

  trigger.addEventListener("click", function () {
    if (panel.hidden) open();
    else close();
  });

  panel.addEventListener("keydown", function (event) {
    if (event.key === "Escape") close();
  });
})();
