/* The banner and its dismissal (CFG-70). A dismissal is remembered against the
   banner's id, so changing the id shows it again. */
(function () {
  var banner = document.querySelector("[data-liyasa='banner']");
  if (!banner) return;
  var id = banner.getAttribute("data-ly-banner-id") || "";
  var key = "liyasa:banner";

  function dismissed() {
    try {
      return window.localStorage.getItem(key);
    } catch (error) {
      return null;
    }
  }

  if (id && dismissed() === id) {
    banner.hidden = true;
    return;
  }

  var button = banner.querySelector("[data-ly-banner-dismiss]");
  if (!button) return;
  button.addEventListener("click", function () {
    banner.hidden = true;
    try {
      window.localStorage.setItem(key, id);
    } catch (error) {
      /* The banner stays dismissed for this page either way. */
    }
    window.liyasa.emit("banner:dismiss", { id: id });
  });
})();
