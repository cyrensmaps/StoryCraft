/*
 * StoryCraft player runtime.
 *
 * Course logic lives in the WebAssembly engine (storycraft-player). This file
 * only moves input into it, draws the frames it returns, and talks to the LMS.
 */
(function () {
  "use strict";

  var course = window.STORYCRAFT_COURSE;
  var target = window.STORYCRAFT_TARGET || "html";
  var SVG_NS = "http://www.w3.org/2000/svg";
  var SUSPEND_LIMIT = 4096; // SCORM 1.2 cmi.suspend_data maximum

  var viewport = document.getElementById("sc-viewport");
  var stage = document.getElementById("sc-stage");
  var nav = document.getElementById("sc-nav");
  var prevButton = document.getElementById("sc-prev");
  var nextButton = document.getElementById("sc-next");
  var positionLabel = document.getElementById("sc-pos");
  var message = document.getElementById("sc-message");

  var width = course.slide_size.width;
  var height = course.slide_size.height;
  var scale = 1;
  var wasm = null;
  var lms = null;
  var stopped = false;

  var mediaUrls = {};
  (course.assets || []).forEach(function (a) {
    mediaUrls[a.id] = "media/" + encodeURIComponent(a.file_name);
  });

  function showMessage(text) {
    message.textContent = text;
    message.hidden = false;
  }

  // --- WebAssembly bridge ----------------------------------------------------

  var encoder = new TextEncoder();
  var decoder = new TextDecoder();

  function decodeBase64(b64) {
    var bin = atob(b64);
    var bytes = new Uint8Array(bin.length);
    for (var i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
    return bytes;
  }

  /** Copy a string into wasm memory; the callee takes ownership. */
  function callWithString(name, s) {
    var bytes = encoder.encode(s);
    var ptr = wasm.sc_alloc(bytes.length);
    new Uint8Array(wasm.memory.buffer, ptr, bytes.length).set(bytes);
    return wasm[name](ptr, bytes.length);
  }

  function readOut(len) {
    return decoder.decode(new Uint8Array(wasm.memory.buffer, wasm.sc_out_ptr(), len));
  }

  // --- LMS (SCORM 1.2) -------------------------------------------------------

  function findApi(win) {
    for (var tries = 0; win && tries < 50; tries++) {
      try {
        if (win.API) return win.API;
      } catch (e) {
        return null; // cross-origin frame
      }
      if (win.parent === win) break;
      win = win.parent;
    }
    return null;
  }

  function timespan(ms) {
    var total = ms / 1000;
    var h = Math.floor(total / 3600);
    var m = Math.floor((total % 3600) / 60);
    var s = total % 60;
    function pad(n, w) {
      var str = String(n);
      while (str.length < w) str = "0" + str;
      return str;
    }
    return pad(Math.min(h, 9999), 4) + ":" + pad(m, 2) + ":" + pad(s.toFixed(2), 5);
  }

  /** Shrink saved progress until it fits the SCORM 1.2 limit. */
  function fitSuspendData(json) {
    if (json.length <= SUSPEND_LIMIT) return json;
    var data = JSON.parse(json);
    var drops = ["states", "visited", "variables"];
    for (var i = 0; i < drops.length; i++) {
      data[drops[i]] = [];
      var out = JSON.stringify(data);
      if (out.length <= SUSPEND_LIMIT) return out;
    }
    return "";
  }

  function connectScorm12() {
    var api = findApi(window) || (window.opener && findApi(window.opener));
    if (!api || String(api.LMSInitialize("")) !== "true") return null;
    var startedAt = Date.now();
    var finished = false;
    var get = function (key) {
      return String(api.LMSGetValue(key) || "");
    };
    var set = function (key, value) {
      api.LMSSetValue(key, String(value));
    };
    var status = get("cmi.core.lesson_status");
    var completed = status === "completed" || status === "passed";
    if (status === "" || status === "not attempted") set("cmi.core.lesson_status", "incomplete");
    api.LMSCommit("");

    return {
      savedProgress: function () {
        return get("cmi.suspend_data");
      },
      save: function (snapshotJson, location) {
        if (finished) return;
        if (location) set("cmi.core.lesson_location", location);
        set("cmi.suspend_data", fitSuspendData(snapshotJson));
        api.LMSCommit("");
      },
      complete: function () {
        if (finished || completed) return;
        completed = true;
        set("cmi.core.lesson_status", "completed");
        api.LMSCommit("");
      },
      finish: function () {
        if (finished) return;
        finished = true;
        set("cmi.core.exit", completed ? "" : "suspend");
        set("cmi.core.session_time", timespan(Date.now() - startedAt));
        api.LMSCommit("");
        api.LMSFinish("");
      }
    };
  }

  function saveProgress(location) {
    if (lms) lms.save(readOut(wasm.sc_snapshot()), location);
  }

  // --- Rendering -------------------------------------------------------------

  function css(color) {
    if (!color) return "transparent";
    if (color.length === 9) {
      var n = parseInt(color.slice(1), 16);
      return "rgba(" + ((n >>> 24) & 255) + "," + ((n >>> 16) & 255) + "," + ((n >>> 8) & 255) + "," +
        ((n & 255) / 255).toFixed(3) + ")";
    }
    return color;
  }

  var nodes = new Map(); // object id -> { el, key, text, svgShape, img }

  function createNode(o, key) {
    var el = document.createElement(o.interactive ? "button" : "div");
    el.className = "sc-obj";
    el.dataset.id = o.id;
    if (o.interactive) el.type = "button";
    var node = { el: el, key: key, text: null, svgShape: null, img: null };

    if (o.kind.type === "shape" && o.kind.shape === "triangle") {
      var svg = document.createElementNS(SVG_NS, "svg");
      svg.setAttribute("class", "sc-fill");
      svg.setAttribute("preserveAspectRatio", "none");
      svg.setAttribute("aria-hidden", "true");
      node.svgShape = document.createElementNS(SVG_NS, "polygon");
      svg.appendChild(node.svgShape);
      el.appendChild(svg);
    }
    if (o.kind.type === "image") {
      node.img = document.createElement("img");
      node.img.className = "sc-fill";
      node.img.draggable = false;
      el.appendChild(node.img);
    }
    node.text = document.createElement("div");
    node.text.className = "sc-text";
    el.appendChild(node.text);
    return node;
  }

  function updateNode(node, o) {
    var el = node.el;
    var s = el.style;
    s.left = o.frame.x + "px";
    s.top = o.frame.y + "px";
    s.width = o.frame.width + "px";
    s.height = o.frame.height + "px";
    s.transform = o.rotation ? "rotate(" + o.rotation + "deg)" : "";
    s.opacity = o.opacity;
    s.cursor = o.interactive ? "pointer" : "";

    if (node.svgShape) {
      var w = o.frame.width, h = o.frame.height, sw = o.stroke ? o.stroke.width : 0;
      node.svgShape.parentNode.setAttribute("viewBox", "0 0 " + w + " " + h);
      node.svgShape.setAttribute("points",
        w / 2 + "," + sw + " " + (w - sw) + "," + (h - sw) + " " + sw + "," + (h - sw));
      node.svgShape.setAttribute("fill", css(o.fill));
      node.svgShape.setAttribute("stroke", o.stroke ? css(o.stroke.color) : "none");
      node.svgShape.setAttribute("stroke-width", sw);
      s.background = "transparent";
      s.border = "none";
    } else {
      s.background = css(o.fill);
      s.border = o.stroke ? o.stroke.width + "px solid " + css(o.stroke.color) : "none";
      s.borderRadius = o.kind.type === "shape" && o.kind.shape === "ellipse" ? "50%" : o.corner_radius + "px";
    }

    if (node.img) {
      var src = mediaUrls[o.image] || "";
      if (node.img.getAttribute("src") !== src) node.img.setAttribute("src", src);
      node.img.alt = o.interactive ? "" : o.alt_text;
    }

    var t = o.text;
    node.text.hidden = !t;
    if (t) {
      if (node.text.textContent !== t.text) node.text.textContent = t.text;
      var ts = node.text.style;
      ts.fontFamily = t.font_family;
      ts.fontSize = t.font_size + "px";
      ts.color = css(t.color);
      ts.fontWeight = t.bold ? "bold" : "normal";
      ts.fontStyle = t.italic ? "italic" : "normal";
      ts.textAlign = t.align;
      ts.justifyContent = { top: "flex-start", middle: "center", bottom: "flex-end" }[t.valign];
    }

    if (o.interactive) {
      var label = o.alt_text || (t ? t.text : "");
      if (label) el.setAttribute("aria-label", label);
      else el.removeAttribute("aria-label");
      el.setAttribute("aria-pressed", o.state === "Selected" ? "true" : "false");
    } else if (o.alt_text && !node.img) {
      el.setAttribute("role", "img");
      el.setAttribute("aria-label", o.alt_text);
    } else {
      el.removeAttribute("role");
      el.removeAttribute("aria-label");
    }
  }

  function render(frame) {
    stage.style.background = css(frame.background);
    var seen = new Set();
    var previous = null;
    frame.objects.forEach(function (o) {
      seen.add(o.id);
      var key = o.kind.type + ":" + (o.kind.shape || "") + ":" + o.interactive;
      var node = nodes.get(o.id);
      if (!node || node.key !== key) {
        if (node) node.el.remove();
        node = createNode(o, key);
        nodes.set(o.id, node);
      }
      updateNode(node, o);
      var expected = previous ? previous.nextSibling : stage.firstChild;
      if (node.el !== expected) stage.insertBefore(node.el, expected);
      previous = node.el;
    });
    nodes.forEach(function (node, id) {
      if (!seen.has(id)) {
        node.el.remove();
        nodes.delete(id);
      }
    });

    prevButton.disabled = !frame.can_go_back;
    nextButton.disabled = !frame.can_go_forward;
    positionLabel.textContent = "Slide " + frame.position[0] + " of " + frame.position[1];
  }

  function layout() {
    var navHeight = nav.hidden ? 0 : nav.offsetHeight;
    var availableHeight = Math.max(window.innerHeight - navHeight, 50);
    scale = Math.min(window.innerWidth / width, availableHeight / height);
    viewport.style.width = width * scale + "px";
    viewport.style.height = height * scale + "px";
    stage.style.width = width + "px";
    stage.style.height = height + "px";
    stage.style.transform = "scale(" + scale + ")";
  }

  // --- Main loop -------------------------------------------------------------

  var lastFrameJson = "";

  function handleEffect(effect) {
    switch (effect.type) {
      case "open_url":
        window.open(effect.value, "_blank", "noopener");
        break;
      case "completed":
        if (lms) lms.complete();
        break;
      case "slide_changed":
        saveProgress(effect.value);
        break;
      case "exit":
        saveProgress(null);
        if (lms) lms.finish();
        stopped = true;
        showMessage("You have left the course. You can close this window.");
        try {
          window.close();
        } catch (e) { /* not a script-opened window */ }
        break;
    }
  }

  function refresh() {
    var json = readOut(wasm.sc_frame());
    if (json !== lastFrameJson) {
      lastFrameJson = json;
      render(JSON.parse(json));
    }
    JSON.parse(readOut(wasm.sc_effects())).forEach(handleEffect);
  }

  var lastTime = 0;
  function loop(now) {
    if (stopped) return;
    // Cap the step so returning to a background tab does not skip a slide.
    var dt = lastTime ? Math.min(now - lastTime, 250) : 0;
    lastTime = now;
    wasm.sc_tick(dt);
    refresh();
    requestAnimationFrame(loop);
  }

  function slidePoint(e) {
    var rect = stage.getBoundingClientRect();
    return [(e.clientX - rect.left) / scale, (e.clientY - rect.top) / scale];
  }

  function bindInput() {
    stage.addEventListener("pointermove", function (e) {
      var p = slidePoint(e);
      wasm.sc_pointer_move(p[0], p[1], 1);
      refresh();
    });
    stage.addEventListener("pointerleave", function () {
      wasm.sc_pointer_move(0, 0, 0);
      refresh();
    });
    stage.addEventListener("pointerdown", function (e) {
      if (e.button !== 0) return;
      var p = slidePoint(e);
      wasm.sc_pointer_down(p[0], p[1]);
      refresh();
    });
    stage.addEventListener("pointerup", function (e) {
      if (e.button !== 0) return;
      var p = slidePoint(e);
      wasm.sc_pointer_up(p[0], p[1]);
      refresh();
    });
    // Keyboard and assistive-technology activation arrives as a click with
    // no pointer detail; pointer clicks are already handled above.
    stage.addEventListener("click", function (e) {
      var el = e.target.closest && e.target.closest("button.sc-obj");
      if (el && e.detail === 0) {
        callWithString("sc_click", el.dataset.id);
        refresh();
      }
    });
    document.addEventListener("keydown", function (e) {
      if (e.repeat || e.ctrlKey || e.metaKey || e.altKey) return;
      var onButton = e.target && e.target.tagName === "BUTTON";
      if (onButton && (e.key === "Enter" || e.key === " ")) return;
      callWithString("sc_key", e.key);
      refresh();
    });
    prevButton.addEventListener("click", function () {
      wasm.sc_prev();
      refresh();
    });
    nextButton.addEventListener("click", function () {
      wasm.sc_next();
      refresh();
    });
    window.addEventListener("resize", layout);
    window.addEventListener("pagehide", function () {
      saveProgress(null);
      if (lms) lms.finish();
    });
  }

  function start() {
    var bytes = decodeBase64(window.STORYCRAFT_WASM);
    return WebAssembly.instantiate(bytes, {}).then(function (result) {
      wasm = result.instance.exports;
      if (callWithString("sc_init", JSON.stringify(course)) !== 0) {
        throw new Error(readOut(wasm.sc_out_len()) || "course failed to load");
      }
      if (target === "scorm12") {
        lms = connectScorm12();
        if (!lms) console.warn("StoryCraft: no SCORM 1.2 LMS found; progress will not be tracked.");
        var saved = lms && lms.savedProgress();
        if (saved) callWithString("sc_restore", saved);
      }
      nav.hidden = !(course.player && course.player.show_nav_buttons);
      document.body.style.background = css(course.player && course.player.backdrop);
      layout();
      bindInput();
      wasm.sc_start();
      refresh();
      requestAnimationFrame(loop);
    });
  }

  start().catch(function (err) {
    console.error(err);
    showMessage("This course could not start: " + (err && err.message ? err.message : err));
  });
})();
