/* Loads a module the first time a reader asks for what it does. The assistant
   panel is budgeted apart from the base bundle (THM-31), so it is fetched on
   the first click and never on a page nobody opens it on. */
(function () {
  var loaded = Object.create(null);

  function load(source) {
    if (!source || loaded[source]) return;
    loaded[source] = true;
    import(source)["catch"](function () {
      loaded[source] = false;
    });
  }

  var assistant = document.querySelector("[data-ly-assistant]");
  if (!assistant) return;
  var source = assistant.getAttribute("data-ly-assistant-module");

  document.querySelectorAll("[data-ly-assistant-trigger]").forEach(function (trigger) {
    trigger.addEventListener(
      "click",
      function () {
        load(source);
      },
      { once: true }
    );
  });
})();
