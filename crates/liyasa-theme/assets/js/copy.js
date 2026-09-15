/* Copy a code block, and the copy confirmation (THM-05). The button is hidden
   until this module reveals it, because without it there is nothing to click. */
(function () {
  document.querySelectorAll("[data-ly-copy]").forEach(function (button) {
    button.hidden = false;
    button.addEventListener("click", function () {
      var target = document.getElementById(button.getAttribute("data-ly-copy"));
      if (!target) return;
      var text = target.textContent || "";
      var done = function () {
        button.setAttribute("data-ly-copied", "true");
        var label = button.getAttribute("data-ly-copied-label") || "Copied";
        var original = button.textContent;
        button.textContent = label;
        window.liyasa.announce(label);
        window.setTimeout(function () {
          button.removeAttribute("data-ly-copied");
          button.textContent = original;
        }, 1600);
      };
      if (navigator.clipboard && navigator.clipboard.writeText) {
        navigator.clipboard.writeText(text).then(done, function () {});
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
        done();
      } catch (error) {
        /* Nothing copied: leave the button alone rather than lie about it. */
      }
      document.body.removeChild(area);
    });
  });
})();
