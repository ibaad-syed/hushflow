// Hushflow site: hero demo, platform toggle, copy buttons. No dependencies.
(function () {
  "use strict";

  var root = document.documentElement;
  var reduceMotion = window.matchMedia(
    "(prefers-reduced-motion: reduce)",
  ).matches;

  /* ---------- Platform (Mac / Windows) ---------- */

  function detectPlatform() {
    var p =
      (navigator.userAgentData && navigator.userAgentData.platform) ||
      navigator.platform ||
      navigator.userAgent ||
      "";
    return /win/i.test(p) ? "win" : "mac";
  }

  function setPlatform(platform) {
    root.setAttribute("data-platform", platform);
    document.querySelectorAll(".platform-toggle button").forEach(function (b) {
      b.setAttribute("aria-pressed", String(b.dataset.platform === platform));
    });
  }

  var detected = detectPlatform();
  setPlatform(detected);

  // Put the visitor's platform first in the hero.
  if (detected === "win") {
    var mac = document.querySelector('.hero [data-dl="mac"]');
    var win = document.querySelector('.hero [data-dl="win"]');
    if (mac && win) {
      // Swap roles: Windows becomes the button, Mac the plain link.
      var icon = mac.querySelector("svg");
      if (icon) icon.remove();
      mac.className = "btn-link";
      win.className = "btn";
      mac.textContent = "Download for Mac";
      win.parentNode.insertBefore(win, mac);
    }
  }

  document.querySelectorAll(".platform-toggle button").forEach(function (b) {
    b.addEventListener("click", function () {
      setPlatform(b.dataset.platform);
    });
  });

  /* ---------- Copy buttons ---------- */

  document.querySelectorAll("[data-copy]").forEach(function (btn) {
    btn.addEventListener("click", function () {
      var el = document.getElementById(btn.dataset.copy);
      if (!el) return;
      var text = el.textContent.trim();
      var done = function () {
        btn.textContent = "Copied";
        setTimeout(function () {
          btn.textContent = "Copy";
        }, 1600);
      };
      if (navigator.clipboard && navigator.clipboard.writeText) {
        navigator.clipboard.writeText(text).then(done, function () {});
      } else {
        var range = document.createRange();
        range.selectNodeContents(el);
        var sel = window.getSelection();
        sel.removeAllRanges();
        sel.addRange(range);
      }
    });
  });

  /* ---------- Hero demo ---------- */

  // Each example: what the speaker says (segments; cut = removed by cleanup),
  // the labels shown while cleaning, and the finished text.
  var EXAMPLES = [
    {
      app: "Messages",
      context: ["bubble", "Are we still on for the review this week?"],
      said: [
        ["um so", true],
        ["can you,", true],
        ["can you send", false],
        ["the deck, no wait,", true],
        ["the notes before Tuesday", false],
      ],
      chips: ["Fillers removed", "Repeat removed", "Correction applied"],
      clean: "Can you send the notes before Tuesday?",
    },
    {
      app: "Notes",
      context: ["title", "Launch checklist"],
      said: [
        ["okay so", true],
        ["for the launch we need", false],
        ["uh", true],
        ["the changelog the screenshots and", false],
        ["um", true],
        ["the download links", false],
      ],
      chips: ["Fillers removed", "Formatted as a list"],
      clean: "For the launch we need:",
      list: ["The changelog", "The screenshots", "The download links"],
    },
    {
      app: "Mail",
      context: ["meta", "To: Sam"],
      said: [
        ["hey sam I'll be there at", false],
        ["like", true],
        ["five, actually make that", true],
        ["six thirty", false],
      ],
      chips: ["Filler removed", "Correction applied"],
      clean: "Hey Sam, I'll be there at 6:30.",
    },
  ];

  var stage = document.getElementById("demo-stage");
  var raw = document.getElementById("demo-raw");
  var chips = document.getElementById("demo-chips");
  var out = document.getElementById("demo-out");
  var pill = document.getElementById("demo-pill");
  var pillText = document.getElementById("pill-text");
  var appName = document.getElementById("demo-app");
  var context = document.getElementById("demo-context");
  var sr = document.getElementById("demo-sr");
  var pauseBtn = document.getElementById("demo-pause");
  var tabs = document.querySelectorAll(".demo-tabs button");

  if (!stage || !raw || !out) return;

  var current = 0;
  var runId = 0;
  var paused = reduceMotion;

  function wait(ms, id) {
    return new Promise(function (resolve, reject) {
      setTimeout(function () {
        if (id === runId) resolve();
        else reject(new Error("cancelled"));
      }, ms);
    });
  }

  function saidText(ex) {
    return ex.said
      .map(function (s) {
        return s[0];
      })
      .join(" ");
  }

  function cleanText(ex) {
    return ex.clean + (ex.list ? " " + ex.list.join(", ") + "." : "");
  }

  function selectTab(i) {
    tabs.forEach(function (t) {
      t.setAttribute("aria-pressed", String(Number(t.dataset.example) === i));
    });
  }

  // Build the raw transcript as individual word spans.
  function buildRaw(ex) {
    raw.textContent = "";
    var words = [];
    ex.said.forEach(function (seg) {
      seg[0].split(" ").forEach(function (w) {
        var span = document.createElement("span");
        span.className = "w";
        span.textContent = w;
        span.dataset.cut = seg[1] ? "1" : "";
        raw.appendChild(span);
        raw.appendChild(document.createTextNode(" "));
        words.push(span);
      });
    });
    return words;
  }

  function buildChips(ex) {
    chips.textContent = "";
    return ex.chips.map(function (label, i) {
      var c = document.createElement("span");
      c.className =
        "chip" + (i === ex.chips.length - 1 && ex.list ? " chip-ok" : "");
      c.textContent = label;
      chips.appendChild(c);
      return c;
    });
  }

  function buildOut(ex) {
    out.textContent = "";
    var p = document.createElement("p");
    p.textContent = ex.clean;
    out.appendChild(p);
    if (ex.list) {
      var ul = document.createElement("ul");
      ex.list.forEach(function (item) {
        var li = document.createElement("li");
        li.textContent = item;
        ul.appendChild(li);
      });
      out.appendChild(ul);
    }
    var caret = document.createElement("span");
    caret.className = "caret";
    (ex.list ? out.querySelector("li:last-child") : p).appendChild(caret);
  }

  function setup(i) {
    var ex = EXAMPLES[i];
    current = i;
    selectTab(i);
    appName.textContent = ex.app;
    if (context) {
      context.className = "context context-" + ex.context[0];
      context.textContent = ex.context[1];
    }
    sr.textContent =
      'Example. You say: "' +
      saidText(ex) +
      '". Hushflow types: "' +
      cleanText(ex) +
      '"';
    stage.textContent = "You say";
    stage.classList.remove("is-done");
    out.classList.remove("in");
    buildOut(ex);
    return { ex: ex, words: buildRaw(ex), chipEls: buildChips(ex) };
  }

  // Jump straight to the finished state (used for reduced motion and pause).
  function showFinal(i) {
    runId++;
    var s = setup(i);
    s.words.forEach(function (w) {
      w.classList.add("in");
      if (w.dataset.cut) w.classList.add("cut");
    });
    s.chipEls.forEach(function (c) {
      c.classList.add("in");
    });
    stage.textContent = "You said";
    stage.classList.add("is-done");
    out.classList.add("in");
    pill.className = "pill is-rest";
  }

  function play(i) {
    var id = ++runId;
    var s = setup(i);

    pill.className = "pill is-live";
    pillText.textContent = "Transcribing...";

    var chain = wait(450, id);
    s.words.forEach(function (w) {
      chain = chain.then(function () {
        w.classList.add("in");
        return wait(150, id);
      });
    });

    chain
      .then(function () {
        return wait(500, id);
      })
      .then(function () {
        pill.className = "pill is-thinking";
        pillText.textContent = "Processing...";
        var marks = wait(200, id);
        s.words.forEach(function (w) {
          if (!w.dataset.cut) return;
          marks = marks.then(function () {
            w.classList.add("cut");
            return wait(110, id);
          });
        });
        s.chipEls.forEach(function (c) {
          marks = marks.then(function () {
            c.classList.add("in");
            return wait(160, id);
          });
        });
        return marks;
      })
      .then(function () {
        return wait(700, id);
      })
      .then(function () {
        pill.className = "pill is-rest";
        stage.textContent = "You said";
        stage.classList.add("is-done");
        out.classList.add("in");
        return wait(4200, id);
      })
      .then(function () {
        if (!paused) play((current + 1) % EXAMPLES.length);
      })
      .catch(function () {
        /* superseded by a newer run */
      });
  }

  function setPaused(value) {
    paused = value;
    pauseBtn.setAttribute("aria-pressed", String(paused));
    pauseBtn.textContent = paused ? "Play" : "Pause";
  }

  tabs.forEach(function (t) {
    t.addEventListener("click", function () {
      var i = Number(t.dataset.example);
      if (paused) showFinal(i);
      else play(i);
    });
  });

  if (reduceMotion) {
    // No autoplay; examples are still browsable with the tabs.
    pauseBtn.hidden = true;
    showFinal(0);
    return;
  }

  pauseBtn.addEventListener("click", function () {
    setPaused(!paused);
    if (paused) showFinal(current);
    else play((current + 1) % EXAMPLES.length);
  });

  // Only animate while the demo is on screen.
  var started = false;
  if ("IntersectionObserver" in window) {
    var io = new IntersectionObserver(function (entries) {
      entries.forEach(function (e) {
        if (e.isIntersecting && !started) {
          started = true;
          play(0);
        }
      });
    });
    io.observe(document.querySelector(".demo"));
  } else {
    play(0);
  }
  showFinal(0);
})();
