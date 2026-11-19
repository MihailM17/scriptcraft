// Theme toggle with persistence. No build step, no dependencies.
//
// The page works without JavaScript: CSS follows prefers-color-scheme. This
// script only remembers an explicit choice and fills in the copyright year.

(function () {
  var root = document.documentElement;
  var KEY = "scriptcraft-theme";

  function stored() {
    try {
      return localStorage.getItem(KEY);
    } catch (e) {
      return null;
    }
  }

  function apply(theme) {
    if (theme === "light" || theme === "dark") {
      root.setAttribute("data-theme", theme);
    } else {
      root.removeAttribute("data-theme");
    }
    var button = document.getElementById("theme-toggle");
    if (button) {
      var dark =
        theme === "dark" ||
        (theme !== "light" &&
          window.matchMedia("(prefers-color-scheme: dark)").matches);
      button.setAttribute("aria-pressed", dark ? "true" : "false");
    }
  }

  apply(stored());

  document.addEventListener("click", function (event) {
    var button = event.target.closest("#theme-toggle");
    if (!button) return;
    var dark = root.getAttribute("data-theme") === "dark";
    if (!root.hasAttribute("data-theme")) {
      dark = window.matchMedia("(prefers-color-scheme: dark)").matches;
    }
    var next = dark ? "light" : "dark";
    apply(next);
    try {
      localStorage.setItem(KEY, next);
    } catch (e) {
      /* private mode: the choice lasts for this page load only */
    }
  });

  var year = document.getElementById("year");
  if (year) {
    year.textContent = String(new Date().getFullYear());
  }
})();
