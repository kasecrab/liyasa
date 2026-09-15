/* Inlined in <head> with the response nonce. Runs before first paint so the
   scheme is right the first time the page is drawn (RX-40). */
(function () {
  var root = document.documentElement;
  root.setAttribute("data-ly-js", "true");
  var strict = root.getAttribute("data-ly-appearance-strict") === "true";
  var fallback = root.getAttribute("data-ly-appearance") || "system";
  var chosen = null;
  if (!strict) {
    try {
      chosen = window.localStorage.getItem("liyasa:theme");
    } catch (error) {
      chosen = null;
    }
  }
  var scheme = chosen || fallback;
  if (scheme === "light" || scheme === "dark") {
    root.setAttribute("data-theme", scheme);
  } else {
    root.removeAttribute("data-theme");
  }
})();
