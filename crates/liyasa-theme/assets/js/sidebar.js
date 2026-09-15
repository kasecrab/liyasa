/* Sidebar groups, keyboard navigation, and the mobile drawer (RX-20, THM-04).
   Groups are open in the markup, so without this module nothing is hidden. */
(function () {
  var storageKey = "liyasa:sidebar";

  function state() {
    try {
      return JSON.parse(window.localStorage.getItem(storageKey) || "{}");
    } catch (error) {
      return {};
    }
  }

  function persist(next) {
    try {
      window.localStorage.setItem(storageKey, JSON.stringify(next));
    } catch (error) {
      /* Nothing to do: the group still collapses for this page. */
    }
  }

  var collapsed = state();

  document.querySelectorAll("[data-ly-group]").forEach(function (header) {
    var id = header.getAttribute("data-ly-group");
    if (collapsed[id]) header.setAttribute("aria-expanded", "false");
    header.addEventListener("click", function () {
      var open = header.getAttribute("aria-expanded") !== "false";
      header.setAttribute("aria-expanded", String(!open));
      collapsed[id] = open;
      persist(collapsed);
    });
  });

  var links = Array.prototype.slice.call(
    document.querySelectorAll('[data-liyasa="sidebar"] .ly-sidebar-link')
  );

  links.forEach(function (link, index) {
    link.addEventListener("keydown", function (event) {
      var next = null;
      if (event.key === "ArrowDown") next = links[index + 1];
      else if (event.key === "ArrowUp") next = links[index - 1];
      else if (event.key === "Home") next = links[0];
      else if (event.key === "End") next = links[links.length - 1];
      if (!next) return;
      event.preventDefault();
      next.focus();
    });
  });

  var active = document.querySelector('.ly-sidebar-link[aria-current="page"]');
  if (active && active.scrollIntoView) {
    active.scrollIntoView({ block: "nearest" });
  }

  /* ---- drawer ---- */
  var drawer = document.querySelector("[data-ly-drawer]");
  var scrim = document.querySelector("[data-ly-scrim]");
  var trigger = document.querySelector("[data-ly-drawer-trigger]");
  if (!drawer || !trigger) return;

  var lastFocus = null;

  function focusable() {
    return Array.prototype.slice.call(
      drawer.querySelectorAll('a[href], button:not([disabled]), select, [tabindex]:not([tabindex="-1"])')
    );
  }

  function open() {
    lastFocus = document.activeElement;
    drawer.hidden = false;
    if (scrim) scrim.hidden = false;
    window.requestAnimationFrame(function () {
      drawer.setAttribute("aria-hidden", "false");
      if (scrim) scrim.setAttribute("aria-hidden", "false");
    });
    trigger.setAttribute("aria-expanded", "true");
    document.body.style.overflow = "hidden";
    var first = focusable()[0];
    if (first) first.focus();
  }

  function close() {
    drawer.setAttribute("aria-hidden", "true");
    if (scrim) scrim.setAttribute("aria-hidden", "true");
    trigger.setAttribute("aria-expanded", "false");
    document.body.style.overflow = "";
    window.setTimeout(function () {
      drawer.hidden = true;
      if (scrim) scrim.hidden = true;
    }, 200);
    if (lastFocus) lastFocus.focus();
  }

  trigger.addEventListener("click", function () {
    if (drawer.hidden) open();
    else close();
  });

  if (scrim) scrim.addEventListener("click", close);

  drawer.addEventListener("keydown", function (event) {
    if (event.key === "Escape") {
      close();
      return;
    }
    if (event.key !== "Tab") return;
    var items = focusable();
    if (items.length === 0) return;
    var first = items[0];
    var last = items[items.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  });
})();
