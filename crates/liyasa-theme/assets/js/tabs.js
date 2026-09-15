/* Tabs: an ARIA tablist with arrow-key navigation (RX-90). Without this module
   every panel is visible with its own heading, which is the readable fallback. */
(function () {
  document.querySelectorAll("[data-ly-tabs]").forEach(function (group) {
    var tabs = Array.prototype.slice.call(group.querySelectorAll('[role="tab"]'));
    var panels = Array.prototype.slice.call(group.querySelectorAll('[role="tabpanel"]'));
    if (tabs.length === 0 || tabs.length !== panels.length) return;

    group.setAttribute("data-ly-enhanced", "true");

    function select(index, focus) {
      tabs.forEach(function (tab, at) {
        var selected = at === index;
        tab.setAttribute("aria-selected", String(selected));
        tab.tabIndex = selected ? 0 : -1;
        panels[at].hidden = !selected;
      });
      if (focus) tabs[index].focus();
      window.liyasa.emit("tabs:change", { group: group.id || null, index: index });
    }

    tabs.forEach(function (tab, index) {
      tab.addEventListener("click", function () {
        select(index, false);
      });
      tab.addEventListener("keydown", function (event) {
        var next = null;
        if (event.key === "ArrowRight") next = (index + 1) % tabs.length;
        else if (event.key === "ArrowLeft") next = (index - 1 + tabs.length) % tabs.length;
        else if (event.key === "Home") next = 0;
        else if (event.key === "End") next = tabs.length - 1;
        if (next === null) return;
        event.preventDefault();
        select(next, true);
      });
    });

    select(0, false);
  });
})();
