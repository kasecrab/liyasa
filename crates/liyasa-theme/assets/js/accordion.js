/* Accordions are <details> elements; this module only adds the height
   transition and the hook, so collapsing works with JavaScript off. */
(function () {
  document.querySelectorAll("[data-ly-accordion]").forEach(function (item) {
    var summary = item.querySelector("summary");
    if (!summary) return;
    summary.addEventListener("click", function () {
      window.liyasa.emit("accordion:toggle", {
        id: item.id || null,
        open: !item.open,
      });
    });
  });
})();
