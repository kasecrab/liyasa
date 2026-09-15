/* The sticky compact header (THM-04). Without it the navbar is simply sticky. */
(function () {
  var navbar = document.querySelector("[data-liyasa='navbar']");
  if (!navbar) return;
  var compact = false;

  function update() {
    var next = window.scrollY > 64;
    if (next === compact) return;
    compact = next;
    navbar.setAttribute("data-ly-compact", String(compact));
  }

  update();
  window.addEventListener("scroll", update, { passive: true });
})();
