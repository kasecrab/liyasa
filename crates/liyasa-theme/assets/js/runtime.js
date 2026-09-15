/* window.liyasa: the documented theme API (CMP-101) and the hook bus (THM-33).
   Loaded before every other module and before any theme.js an operator adds,
   so a custom script can register a hook and still see page:load. */
(function () {
  var handlers = Object.create(null);
  var metadata = {};
  var element = document.getElementById("ly-page-data");
  if (element && element.textContent) {
    try {
      metadata = JSON.parse(element.textContent);
    } catch (error) {
      metadata = {};
    }
  }

  function on(name, handler) {
    if (typeof handler !== "function") return function () {};
    (handlers[name] || (handlers[name] = [])).push(handler);
    return function off() {
      liyasa.off(name, handler);
    };
  }

  function off(name, handler) {
    var list = handlers[name];
    if (!list) return;
    var at = list.indexOf(handler);
    if (at !== -1) list.splice(at, 1);
  }

  function emit(name, detail) {
    var payload = detail === undefined ? {} : detail;
    (handlers[name] || []).slice().forEach(function (handler) {
      try {
        handler(payload);
      } catch (error) {
        /* One bad handler never takes the page with it. */
        if (window.console) console.error("liyasa hook " + name, error);
      }
    });
    document.dispatchEvent(
      new CustomEvent("liyasa:" + name, { detail: payload, bubbles: true })
    );
  }

  var liyasa = {
    version: metadata.version || null,
    page: metadata.page || {},
    site: metadata.site || {},
    reader: metadata.reader || {},
    playground: metadata.playground || {},
    on: on,
    off: off,
    emit: emit,
    /* Theme control, the same entry point the toggle uses (RX-40). */
    theme: {
      get: function () {
        return document.documentElement.getAttribute("data-theme") || "system";
      },
      set: function (scheme) {
        emit("theme:request", { scheme: scheme });
      },
      toggle: function () {
        emit("theme:request", { scheme: "toggle" });
      },
    },
    announce: function (message) {
      var live = document.getElementById("ly-live-region");
      if (!live) return;
      live.textContent = "";
      window.setTimeout(function () {
        live.textContent = message;
      }, 50);
    },
  };

  window.liyasa = liyasa;

  function ready() {
    emit("page:load", { page: liyasa.page });
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", ready, { once: true });
  } else {
    ready();
  }

  /* A route change announced to assistive technology (RX-90). */
  window.addEventListener("popstate", function () {
    emit("page:load", { page: liyasa.page });
    liyasa.announce(document.title);
  });
})();
