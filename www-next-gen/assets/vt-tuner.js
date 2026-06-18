// View-transition tuner: a small dialog to play with the page-transition dials.
// Settings persist in localStorage and are applied to <html> on every page load
// (this script lives in <head>), so cross-document navigations use them.
(function () {
  var KEY = "vt-tuner";
  var EASES = {
    standard: "var(--anim-ease-standard)",
    emphasized: "var(--anim-ease-emphasized)",
    entrance: "var(--anim-ease-entrance)",
    bounce: "var(--anim-ease-bounce)",
    linear: "linear",
  };
  var STYLES = ["crossfade", "fadein", "slide"];
  var DEFAULTS = { enabled: true, duration: 120, ease: "standard", style: "crossfade", scope: false, help: false };

  function load() {
    try {
      return Object.assign({}, DEFAULTS, JSON.parse(localStorage.getItem(KEY) || "{}"));
    } catch (e) {
      return Object.assign({}, DEFAULTS);
    }
  }
  function save() {
    localStorage.setItem(KEY, JSON.stringify(state));
  }
  // the panel's open/closed state persists too, so it stays open as you navigate
  // between pages to compare transitions.
  var OPEN_KEY = "vt-tuner-open";
  function setOpen(v) {
    try {
      if (v) localStorage.setItem(OPEN_KEY, "1");
      else localStorage.removeItem(OPEN_KEY);
    } catch (e) {}
  }
  function isOpen() {
    return localStorage.getItem(OPEN_KEY) === "1";
  }
  function apply() {
    var r = document.documentElement;
    r.style.setProperty("--vt-duration", state.duration + "ms");
    r.style.setProperty("--vt-ease", EASES[state.ease] || EASES.standard);
    r.classList.toggle("vt-off", !state.enabled);
    r.classList.toggle("vt-fadein", state.style === "fadein");
    r.classList.toggle("vt-slide", state.style === "slide");
    r.classList.toggle("vt-scope", !!state.scope);
  }

  var state = load();
  apply(); // before paint

  function opts(list) {
    return list
      .map(function (k) {
        return '<option value="' + k + '">' + k + "</option>";
      })
      .join("");
  }

  function build() {
    if (document.getElementById("vt-tuner")) return;
    var btn = document.getElementById("vt-tuner-btn");
    if (!btn) return; // the nav launcher is server-rendered

    var dlg = document.createElement("dialog");
    dlg.id = "vt-tuner";
    dlg.innerHTML = [
      '<div class="vt-head"><h2>Page transition</h2>',
      '<button type="button" class="vt-help-toggle" data-act="help" title="What do these do?"><iconify-icon icon="lucide:help-circle" width="18" height="18"></iconify-icon></button></div>',
      '<div class="vt-row"><label><input type="checkbox" data-k="enabled"> Enabled</label></div>',
      '<p class="vt-help">Turns the page-to-page animation on or off.</p>',
      '<div class="vt-row"><label>Duration</label><input type="range" min="0" max="600" step="10" data-k="duration"><output data-out="duration"></output></div>',
      '<p class="vt-help">How long the transition lasts. Shorter feels snappier.</p>',
      '<div class="vt-row"><label>Easing</label><select data-k="ease">' + opts(Object.keys(EASES)) + "</select></div>",
      '<p class="vt-help">The speed curve. Standard is even; bounce overshoots playfully.</p>',
      '<div class="vt-row"><label>Style</label><select data-k="style">' + opts(STYLES) + "</select></div>",
      '<p class="vt-help">Cross-fade blends the pages; fade-in holds the old page and fades the new one in; slide adds a small upward rise.</p>',
      '<div class="vt-row"><label><input type="checkbox" data-k="scope"> Hold nav + chrome still</label></div>',
      '<p class="vt-help">Keeps the nav anchored so only the content transitions, instead of the whole page fading at once.</p>',
      '<p class="vt-hint">Dials apply to real page navigations. Open one to see it:</p>',
      '<div class="vt-tryto"><a href="/themes">Themes</a><a href="/reference">Reference</a><a href="/">Home</a></div>',
      '<div class="vt-actions"><button type="button" data-act="reset">Reset</button><button type="button" data-act="close">Close</button></div>',
    ].join("");

    document.body.appendChild(dlg);

    function sync() {
      dlg.querySelectorAll("[data-k]").forEach(function (el) {
        var k = el.dataset.k;
        if (el.type === "checkbox") el.checked = !!state[k];
        else el.value = state[k];
      });
      var out = dlg.querySelector('[data-out="duration"]');
      if (out) out.textContent = state.duration + "ms";
    }
    function applyHelp() {
      dlg.classList.toggle("show-help", !!state.help);
      var t = dlg.querySelector(".vt-help-toggle");
      if (t) t.setAttribute("aria-pressed", state.help ? "true" : "false");
    }

    dlg.addEventListener("input", function (e) {
      var el = e.target;
      if (!el.dataset || !el.dataset.k) return;
      var k = el.dataset.k;
      state[k] = el.type === "checkbox" ? el.checked : k === "duration" ? +el.value : el.value;
      apply();
      save();
      sync();
    });
    dlg.addEventListener("click", function (e) {
      var actEl = e.target.closest ? e.target.closest("[data-act]") : null;
      var act = actEl ? actEl.dataset.act : null;
      if (act === "close") dlg.close();
      if (act === "help") {
        state.help = !state.help;
        save();
        applyHelp();
      }
      if (act === "reset") {
        state = Object.assign({}, DEFAULTS);
        apply();
        save();
        sync();
        applyHelp();
      }
    });
    function openPanel() {
      sync();
      setOpen(true);
      btn.setAttribute("aria-pressed", "true");
      if (!dlg.open) dlg.show(); // non-modal: no backdrop, so the page transition behind stays visible
    }
    dlg.addEventListener("close", function () {
      setOpen(false);
      btn.setAttribute("aria-pressed", "false");
    });
    document.addEventListener("keydown", function (e) {
      if (e.key === "Escape" && dlg.open) dlg.close();
    });
    // light dismiss: click anywhere outside the panel (but not the launcher) closes it
    document.addEventListener("click", function (e) {
      if (dlg.open && !dlg.contains(e.target) && !btn.contains(e.target)) dlg.close();
    });
    btn.addEventListener("click", function () {
      if (dlg.open) dlg.close();
      else openPanel();
    });
    sync();
    applyHelp();
    if (isOpen()) openPanel();
  }

  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", build);
  else build();
})();
