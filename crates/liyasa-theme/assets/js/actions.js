/* The page actions menu (RX-100). Without this module the links are still in
   the markup; the menu simply does not collapse. */
(function () {
  document.querySelectorAll("[data-liyasa='page-actions']").forEach(function (group) {
    var trigger = group.querySelector("[data-ly-actions-trigger]");
    var menu = group.querySelector(".ly-page-actions-menu");
    if (!trigger || !menu) return;

    function open(next) {
      menu.hidden = !next;
      trigger.setAttribute("aria-expanded", String(next));
    }

    trigger.addEventListener("click", function () {
      open(menu.hidden);
    });

    menu.addEventListener("click", function (event) {
      if (event.target.closest("a, button")) open(false);
    });

    document.addEventListener("click", function (event) {
      if (!group.contains(event.target)) open(false);
    });

    group.addEventListener("keydown", function (event) {
      if (event.key !== "Escape") return;
      open(false);
      trigger.focus();
    });
  });
})();
