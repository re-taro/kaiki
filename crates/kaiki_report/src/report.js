(function () {
  "use strict";

  var data = JSON.parse(document.getElementById("reg-data").textContent);
  var app = document.getElementById("app");

  var tabs = [
    { key: "failed", label: "Failed", items: data.failedItems, dir: data.diffDir },
    { key: "new", label: "New", items: data.newItems, dir: data.actualDir },
    { key: "deleted", label: "Deleted", items: data.deletedItems, dir: data.expectedDir },
    { key: "passed", label: "Passed", items: data.passedItems, dir: data.diffDir },
  ];

  // Find initial active tab (first non-empty, or "failed")
  var activeTab = "failed";
  for (var i = 0; i < tabs.length; i++) {
    if (tabs[i].items.length > 0) {
      activeTab = tabs[i].key;
      break;
    }
  }

  function esc(s) {
    var d = document.createElement("div");
    d.appendChild(document.createTextNode(s));
    return d.innerHTML;
  }

  // Build header + summary
  var html = '<div class="header"><h1>reg report</h1><div class="summary">';
  for (var i = 0; i < tabs.length; i++) {
    var t = tabs[i];
    var cls = "badge badge-" + t.key + (t.key === activeTab ? " active" : "");
    html += '<span class="' + cls + '" data-tab="' + t.key + '">';
    html += t.label + ": " + t.items.length + "</span>";
  }
  html += "</div></div>";

  // Build content panels
  html += '<div class="content">';

  for (var i = 0; i < tabs.length; i++) {
    var t = tabs[i];
    var isActive = t.key === activeTab ? " active" : "";
    html += '<div class="tab-panel' + isActive + '" id="panel-' + t.key + '">';

    if (t.items.length === 0) {
      html += '<div class="empty">No ' + t.label.toLowerCase() + " items</div>";
    } else {
      html += '<div class="card-list">';
      for (var j = 0; j < t.items.length; j++) {
        var name = t.items[j];
        html += '<div class="card" data-index="' + j + '">';
        html += '<div class="card-header">' + esc(name) + "</div>";
        html += '<div class="card-body">';

        if (t.key === "failed") {
          // Triple layout: expected | diff | actual
          html += '<div class="triple">';
          html += '<div class="triple-col"><div class="label">Expected</div>';
          html += '<img src="' + esc(data.expectedDir + "/" + name) + '" loading="lazy" alt="expected"></div>';
          html += '<div class="triple-col"><div class="label">Diff</div>';
          html += '<img src="' + esc(data.diffDir + "/" + name) + '" loading="lazy" alt="diff"></div>';
          html += '<div class="triple-col"><div class="label">Actual</div>';
          html += '<img src="' + esc(data.actualDir + "/" + name) + '" loading="lazy" alt="actual"></div>';
          html += "</div>";
          // Slider
          html += '<div class="slider-container" style="--slider-pos:50%">';
          html += '<img src="' + esc(data.actualDir + "/" + name) + '" loading="lazy" alt="actual">';
          html += '<div class="slider-overlay">';
          html += '<img src="' + esc(data.expectedDir + "/" + name) + '" loading="lazy" alt="expected">';
          html += "</div>";
          html += '<div class="slider-handle"></div>';
          html += "</div>";
          html += '<div class="slider-labels"><span>Expected</span><span>Actual</span></div>';
        } else if (t.key === "new") {
          html += '<img src="' + esc(data.actualDir + "/" + name) + '" loading="lazy" alt="actual">';
        } else if (t.key === "deleted") {
          html += '<img src="' + esc(data.expectedDir + "/" + name) + '" loading="lazy" alt="expected">';
        } else {
          // Passed — show diff if available
          html += '<img src="' + esc(data.diffDir + "/" + name) + '" loading="lazy" alt="diff">';
        }

        html += "</div></div>";
      }
      html += "</div>";
    }

    html += "</div>";
  }

  html += "</div>";

  // Zoom modal
  html += '<div class="modal-overlay" id="zoom-modal"><img src="" alt="zoom"></div>';

  app.innerHTML = html;

  // Tab switching
  var badges = document.querySelectorAll(".badge[data-tab]");
  for (var i = 0; i < badges.length; i++) {
    badges[i].addEventListener("click", function () {
      var tab = this.getAttribute("data-tab");
      for (var j = 0; j < badges.length; j++) {
        badges[j].classList.toggle("active", badges[j] === this);
      }
      var panels = document.querySelectorAll(".tab-panel");
      for (var j = 0; j < panels.length; j++) {
        panels[j].classList.toggle("active", panels[j].id === "panel-" + tab);
      }
    });
  }

  // Slider drag
  document.addEventListener("mousedown", function (e) {
    var container = e.target.closest(".slider-container");
    if (!container) return;
    e.preventDefault();

    function update(ev) {
      var rect = container.getBoundingClientRect();
      var x = Math.max(0, Math.min(ev.clientX - rect.left, rect.width));
      var pct = (x / rect.width) * 100;
      container.style.setProperty("--slider-pos", pct + "%");
    }

    update(e);

    function onMove(ev) { update(ev); }
    function onUp() {
      document.removeEventListener("mousemove", onMove);
      document.removeEventListener("mouseup", onUp);
    }
    document.addEventListener("mousemove", onMove);
    document.addEventListener("mouseup", onUp);
  });

  // Touch slider
  document.addEventListener("touchstart", function (e) {
    var container = e.target.closest(".slider-container");
    if (!container) return;

    function update(ev) {
      var touch = ev.touches[0];
      var rect = container.getBoundingClientRect();
      var x = Math.max(0, Math.min(touch.clientX - rect.left, rect.width));
      var pct = (x / rect.width) * 100;
      container.style.setProperty("--slider-pos", pct + "%");
    }

    update(e);

    function onMove(ev) { ev.preventDefault(); update(ev); }
    function onEnd() {
      document.removeEventListener("touchmove", onMove);
      document.removeEventListener("touchend", onEnd);
    }
    document.addEventListener("touchmove", onMove, { passive: false });
    document.addEventListener("touchend", onEnd);
  }, { passive: true });

  // Sync overlay image width after load
  document.addEventListener("load", function (e) {
    if (e.target.tagName !== "IMG") return;
    var overlay = e.target.closest(".slider-overlay");
    if (!overlay) return;
    var container = overlay.closest(".slider-container");
    if (container) {
      overlay.querySelector("img").style.width = container.offsetWidth + "px";
    }
  }, true);

  // Zoom modal
  var modal = document.getElementById("zoom-modal");
  var modalImg = modal.querySelector("img");

  document.addEventListener("click", function (e) {
    if (e.target.tagName === "IMG" && e.target.closest(".card-body") && !e.target.closest(".slider-container")) {
      modalImg.src = e.target.src;
      modal.classList.add("active");
    }
  });

  modal.addEventListener("click", function () {
    modal.classList.remove("active");
    modalImg.src = "";
  });

  // Keyboard navigation
  document.addEventListener("keydown", function (e) {
    // Escape closes zoom
    if (e.key === "Escape" && modal.classList.contains("active")) {
      modal.classList.remove("active");
      modalImg.src = "";
      return;
    }

    // j/k navigate cards
    if (e.key === "j" || e.key === "k") {
      var activePanel = document.querySelector(".tab-panel.active");
      if (!activePanel) return;
      var cards = activePanel.querySelectorAll(".card");
      if (cards.length === 0) return;

      var current = activePanel.querySelector(".card.highlight");
      var idx = current ? Array.prototype.indexOf.call(cards, current) : -1;

      if (e.key === "j") idx = Math.min(idx + 1, cards.length - 1);
      else idx = Math.max(idx - 1, 0);

      if (current) current.classList.remove("highlight");
      cards[idx].classList.add("highlight");
      cards[idx].scrollIntoView({ behavior: "smooth", block: "nearest" });
    }
  });

  // Wasm bounding box integration
  document.addEventListener("kaiki-wasm-ready", function () {
    var wasm = window.__kaikiWasm;
    if (!wasm) return;
    analyzeFailed(wasm, data);
  });

  function loadImageData(src) {
    return new Promise(function (resolve, reject) {
      var img = new Image();
      img.crossOrigin = "anonymous";
      img.onload = function () {
        var canvas = document.createElement("canvas");
        canvas.width = img.naturalWidth;
        canvas.height = img.naturalHeight;
        var ctx = canvas.getContext("2d");
        ctx.drawImage(img, 0, 0);
        var imageData = ctx.getImageData(0, 0, canvas.width, canvas.height);
        resolve({ data: new Uint8Array(imageData.data.buffer), width: canvas.width, height: canvas.height });
      };
      img.onerror = reject;
      img.src = src;
    });
  }

  function analyzeFailed(wasm, d) {
    for (var i = 0; i < d.failedItems.length; i++) {
      (function (name) {
        Promise.all([
          loadImageData(d.actualDir + "/" + name),
          loadImageData(d.expectedDir + "/" + name)
        ]).then(function (imgs) {
          var actual = imgs[0];
          var expected = imgs[1];
          var w = Math.max(actual.width, expected.width);
          var h = Math.max(actual.height, expected.height);
          var result = wasm.comparePixelsWithRegions(actual.data, expected.data, w, h, 0.0, 16);
          if (result && result.regions) {
            drawRegions(name, result.regions, w, h);
          }
        }).catch(function () { /* ignore load failures */ });
      })(d.failedItems[i]);
    }
  }

  function drawRegions(name, regions, imgWidth, imgHeight) {
    if (regions.length === 0) return;

    // Find the matching card by looking for the card with this filename
    var cards = document.querySelectorAll("#panel-failed .card");
    for (var i = 0; i < cards.length; i++) {
      var header = cards[i].querySelector(".card-header");
      if (!header || header.textContent !== name) continue;

      var tripleCols = cards[i].querySelectorAll(".triple-col");
      for (var j = 0; j < tripleCols.length; j++) {
        var label = tripleCols[j].querySelector(".label");
        if (!label) continue;
        var text = label.textContent;
        if (text !== "Expected" && text !== "Actual") continue;

        var svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
        svg.setAttribute("class", "bbox-overlay");
        svg.setAttribute("viewBox", "0 0 " + imgWidth + " " + imgHeight);
        svg.setAttribute("preserveAspectRatio", "xMidYMid meet");

        for (var k = 0; k < regions.length; k++) {
          var r = regions[k];
          var rect = document.createElementNS("http://www.w3.org/2000/svg", "rect");
          rect.setAttribute("x", r.x);
          rect.setAttribute("y", r.y);
          rect.setAttribute("width", r.width);
          rect.setAttribute("height", r.height);
          svg.appendChild(rect);
        }

        tripleCols[j].appendChild(svg);
      }
      break;
    }
  }
})();
