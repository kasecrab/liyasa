/* Prefetch in-site links on hover or intersection, and honour Save-Data. */
(function () {
  var connection = navigator.connection || {};
  if (connection.saveData) return;
  if (window.matchMedia("(prefers-reduced-data: reduce)").matches) return;

  var seen = Object.create(null);

  function prefetch(href) {
    if (!href || seen[href]) return;
    seen[href] = true;
    var link = document.createElement("link");
    link.rel = "prefetch";
    link.href = href;
    document.head.appendChild(link);
  }

  function isLocal(link) {
    return link.origin === window.location.origin && !link.hasAttribute("download");
  }

  document.addEventListener(
    "pointerenter",
    function (event) {
      var link = event.target.closest && event.target.closest("a[href]");
      if (link && isLocal(link)) prefetch(link.href);
    },
    true
  );

  if (!("IntersectionObserver" in window)) return;
  var observer = new IntersectionObserver(function (entries) {
    entries.forEach(function (entry) {
      if (!entry.isIntersecting) return;
      observer.unobserve(entry.target);
      if (isLocal(entry.target)) prefetch(entry.target.href);
    });
  });
  document
    .querySelectorAll('[data-liyasa="sidebar"] a[href], [data-liyasa="pagination"] a[href]')
    .forEach(function (link) {
      observer.observe(link);
    });
})();
