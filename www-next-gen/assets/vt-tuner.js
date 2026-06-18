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
  var DEFAULTS = { enabled: true, duration: 120, ease: "standard", style: "crossfade", scope: false };

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
    if (document.getElementById("vt-tuner-btn")) return;

    var btn = document.createElement("button");
    btn.id = "vt-tuner-btn";
    btn.type = "button";
    btn.title = "View transition tuner";
    btn.innerHTML =
      '<iconify-icon icon="lucide:sliders-horizontal" width="20" height="20"></iconify-icon>';

    var dlg = document.createElement("dialog");
    dlg.id = "vt-tuner";
    dlg.innerHTML = [
      "<h2>Page transition</h2>",
      '<div class="vt-row"><label><input type="checkbox" data-k="enabled"> Enabled</label></div>',
      '<div class="vt-row"><label>Duration</label><input type="range" min="0" max="600" step="10" data-k="duration"><output data-out="duration"></output></div>',
      '<div class="vt-row"><label>Easing</label><select data-k="ease">' + opts(Object.keys(EASES)) + "</select></div>",
      '<div class="vt-row"><label>Style</label><select data-k="style">' + opts(STYLES) + "</select></div>",
      '<div class="vt-row"><label><input type="checkbox" data-k="scope"> Hold nav + chrome still</label></div>',
      '<div class="vt-tryto">Feel it: <a href="/themes">Themes</a><a href="/reference">Reference</a><a href="/">Home</a></div>',
      '<div class="vt-actions"><button type="button" data-act="reset">Reset</button><button type="button" data-act="close">Close</button></div>',
    ].join("");

    document.body.appendChild(btn);
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
      var act = e.target.dataset ? e.target.dataset.act : null;
      if (act === "close") dlg.close();
      if (act === "reset") {
        state = Object.assign({}, DEFAULTS);
        apply();
        save();
        sync();
      }
    });
    btn.addEventListener("click", function () {
      sync();
      dlg.showModal();
    });
    sync();
  }

  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", build);
  else build();
})();
