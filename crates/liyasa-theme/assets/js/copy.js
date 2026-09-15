/* Copy a code block or a page action, and the copy confirmation (THM-05).
   Buttons are hidden until this module reveals them, because without it there
   is nothing for them to do. */
(function () {
  function confirm(button) {
    var label = button.getAttribute("data-ly-copied-label") || "Copied";
    var original = button.textContent;
    button.setAttribute("data-ly-copied", "true");
    button.textContent = label;
    window.liyasa.announce(label);
    window.setTimeout(function () {
      button.removeAttribute("data-ly-copied");
      button.textContent = original;
    }, 1600);
  }

  function write(text, button) {
    if (navigator.clipboard && navigator.clipboard.writeText) {
      navigator.clipboard.writeText(text).then(function () {
        confirm(button);
      }, function () {});
      return;
    }
    var area = document.createElement("textarea");
    area.value = text;
    area.setAttribute("readonly", "");
    area.style.position = "absolute";
    area.style.left = "-9999px";
    document.body.appendChild(area);
    area.select();
    try {
      document.execCommand("copy");
      confirm(button);
    } catch (error) {
      /* Nothing copied: leave the button alone rather than lie about it. */
    }
    document.body.removeChild(area);
  }

  document.querySelectorAll("[data-ly-copy], [data-ly-copy-url]").forEach(function (button) {
    button.hidden = false;
    button.addEventListener("click", function () {
      /* The page's Markdown is fetched rather than inlined (RX-14). */
      var url = button.getAttribute("data-ly-copy-url");
      if (url && window.fetch) {
        window
          .fetch(url, { credentials: "same-origin" })
          .then(function (response) {
            return response.text();
          })
          .then(function (text) {
            write(text, button);
          })["catch"](function () {});
        return;
      }
      var target = document.getElementById(button.getAttribute("data-ly-copy"));
      if (target) write(target.textContent || "", button);
    });
  });
})();
