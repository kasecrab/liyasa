/* Active-section tracking and back to top (RX-21). The table of contents is a
   list of links in the markup, so it works without this module. */
(function () {
  var links = Array.prototype.slice.call(document.querySelectorAll(".ly-toc-link"));
  if (links.length === 0 || !("IntersectionObserver" in window)) return;

  var byId = {};
  var headings = [];
  links.forEach(function (link) {
    var id = decodeURIComponent((link.getAttribute("href") || "").slice(1));
    var heading = id && document.getElementById(id);
    if (!heading) return;
    byId[id] = link;
    headings.push(heading);
  });

  var current = null;

  function mark(id) {
    if (current === id) return;
    current = id;
    links.forEach(function (link) {
      link.removeAttribute("aria-current");
    });
    if (byId[id]) byId[id].setAttribute("aria-current", "true");
  }

  var observer = new IntersectionObserver(
    function (entries) {
      var visible = entries
        .filter(function (entry) {
          return entry.isIntersecting;
        })
        .sort(function (a, b) {
          return a.boundingClientRect.top - b.boundingClientRect.top;
        });
      if (visible.length > 0) mark(visible[0].target.id);
    },
    { rootMargin: "-10% 0px -70% 0px", threshold: 0 }
  );

  headings.forEach(function (heading) {
    observer.observe(heading);
  });

  var top = document.querySelector("[data-ly-back-to-top]");
  if (top) {
    top.hidden = false;
    top.addEventListener("click", function (event) {
      event.preventDefault();
      window.scrollTo({ top: 0, behavior: "smooth" });
      var main = document.getElementById("ly-main");
      if (main) main.focus();
    });
  }
})();
