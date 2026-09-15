/* The appearance toggle (RX-40). Without this module the page still renders in
   the reader's system scheme; the toggle is simply absent. */
(function () {
  var root = document.documentElement;
  var storageKey = "liyasa:theme";
  var media = window.matchMedia("(prefers-color-scheme: dark)");

  function stored() {
    try {
      return window.localStorage.getItem(storageKey);
    } catch (error) {
      return null;
    }
  }

  function remember(scheme) {
    try {
      if (scheme === "system") window.localStorage.removeItem(storageKey);
      else window.localStorage.setItem(storageKey, scheme);
    } catch (error) {
      /* Private browsing: the choice lasts for this page, which is fine. */
    }
  }

  function resolved() {
    var attribute = root.getAttribute("data-theme");
    if (attribute) return attribute;
    return media.matches ? "dark" : "light";
  }

  function apply(scheme) {
    if (scheme === "system") root.removeAttribute("data-theme");
    else root.setAttribute("data-theme", scheme);
    remember(scheme);
    document.querySelectorAll("[data-ly-theme-toggle]").forEach(function (button) {
      button.setAttribute("aria-pressed", String(resolved() === "dark"));
    });
    window.liyasa.emit("theme:change", { scheme: scheme, resolved: resolved() });
  }

  if (root.getAttribute("data-ly-appearance-strict") === "true") return;

  document.querySelectorAll("[data-ly-theme-toggle]").forEach(function (button) {
    button.hidden = false;
    button.setAttribute("aria-pressed", String(resolved() === "dark"));
    button.addEventListener("click", function () {
      apply(resolved() === "dark" ? "light" : "dark");
    });
  });

  window.liyasa.on("theme:request", function (detail) {
    var scheme = detail.scheme;
    if (scheme === "toggle") scheme = resolved() === "dark" ? "light" : "dark";
    apply(scheme);
  });

  media.addEventListener("change", function () {
    if (!stored()) apply("system");
  });
})();
