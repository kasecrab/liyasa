/* Page feedback. The form posts without JavaScript; this module keeps the
   reader on the page and emits the hook (THM-33). */
(function () {
  var form = document.querySelector("[data-ly-feedback]");
  if (!form) return;

  form.addEventListener("submit", function (event) {
    var button = document.activeElement;
    var helpful = button && button.getAttribute("data-ly-feedback-value");
    if (!helpful) return;
    event.preventDefault();
    var detail = {
      page: window.liyasa.page.route || window.location.pathname,
      helpful: helpful === "yes",
    };
    window.liyasa.emit("feedback:submit", detail);
    var endpoint = form.getAttribute("action");
    if (endpoint && window.fetch) {
      window.fetch(endpoint, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(detail),
        credentials: "same-origin",
      })["catch"](function () {});
    }
    var thanks = form.getAttribute("data-ly-feedback-thanks") || "Thank you";
    form.innerHTML = "";
    form.textContent = thanks;
    window.liyasa.announce(thanks);
  });
})();
