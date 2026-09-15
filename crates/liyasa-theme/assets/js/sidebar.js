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

  /* ---- drawer ----
     The sidebar is the drawer, and `#ly-sidebar` opens it without this module;
     what the module adds is the scrim, the focus trap, and Escape. */
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
    if (scrim) scrim.hidden = false;
    drawer.setAttribute("data-ly-open", "true");
    if (scrim) scrim.setAttribute("aria-hidden", "false");
    trigger.setAttribute("aria-expanded", "true");
    document.body.style.overflow = "hidden";
    var first = focusable()[0];
    if (first) first.focus();
  }

  function close() {
    drawer.removeAttribute("data-ly-open");
    if (scrim) scrim.setAttribute("aria-hidden", "true");
    trigger.setAttribute("aria-expanded", "false");
    document.body.style.overflow = "";
    window.setTimeout(function () {
      if (scrim) scrim.hidden = true;
    }, 200);
    if (lastFocus) lastFocus.focus();
    if (window.location.hash === "#ly-sidebar") {
      window.history.replaceState(null, "", window.location.pathname + window.location.search);
    }
  }

  trigger.addEventListener("click", function (event) {
    event.preventDefault();
    if (drawer.getAttribute("data-ly-open") === "true") close();
    else open();
  });

  var closer = drawer.querySelector("[data-ly-drawer-close]");
  if (closer) {
    closer.addEventListener("click", function (event) {
      event.preventDefault();
      close();
    });
  }

  if (scrim) scrim.addEventListener("click", close);

  document.addEventListener("keydown", function (event) {
    if (event.key === "Escape" && drawer.getAttribute("data-ly-open") === "true") close();
  });

  drawer.addEventListener("keydown", function (event) {
    if (event.key !== "Tab") return;
    if (drawer.getAttribute("data-ly-open") !== "true") return;
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
