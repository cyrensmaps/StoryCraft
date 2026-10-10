/*
 * StoryCraft LMS adapters: SCORM 1.2, SCORM 2004 (4th edition), xAPI and cmi5.
 *
 * Every adapter has the same shape, so the player does not care which
 * standard it is talking to:
 *
 *   saved                        progress saved by a previous session ("" if none)
 *   save(snapshotJson, location) store progress (location = current slide id)
 *   slideViewed(id, name)        a slide was shown
 *   complete()                   the course is complete
 *   finish()                     the session is ending
 *
 * window.StoryCraftLms.connect(target, info) resolves to an adapter, or to
 * null when no LMS is reachable (the course still runs, untracked).
 */
(function () {
  "use strict";

  function warn(msg) {
    console.warn("StoryCraft: " + msg);
  }

  function pad(n, width) {
    var s = String(n);
    while (s.length < width) s = "0" + s;
    return s;
  }

  /** ISO 8601 duration, e.g. PT1H2M3.45S (SCORM 2004, xAPI). */
  function isoDuration(ms) {
    var total = Math.max(ms, 0) / 1000;
    var h = Math.floor(total / 3600);
    var m = Math.floor((total % 3600) / 60);
    var s = (total % 60).toFixed(2);
    return "PT" + h + "H" + m + "M" + s + "S";
  }

  /** SCORM 1.2 CMITimespan, HHHH:MM:SS.SS. */
  function scorm12Duration(ms) {
    var total = Math.max(ms, 0) / 1000;
    var h = Math.floor(total / 3600);
    var m = Math.floor((total % 3600) / 60);
    var s = total % 60;
    return pad(Math.min(h, 9999), 4) + ":" + pad(m, 2) + ":" + pad(s.toFixed(2), 5);
  }

  /** Shrink saved progress until it fits `limit` characters. */
  function fitSnapshot(json, limit) {
    if (json.length <= limit) return json;
    var data = JSON.parse(json);
    var drops = ["states", "visited", "variables"];
    for (var i = 0; i < drops.length; i++) {
      data[drops[i]] = [];
      var out = JSON.stringify(data);
      if (out.length <= limit) return out;
    }
    return "";
  }

  /** Look for a SCORM API object in this window, its parents, and the opener's. */
  function findApi(name) {
    function search(win) {
      for (var tries = 0; win && tries < 50; tries++) {
        try {
          if (win[name]) return win[name];
        } catch (e) {
          return null; // cross-origin frame
        }
        if (win.parent === win) break;
        win = win.parent;
      }
      return null;
    }
    return search(window) || (window.opener ? search(window.opener) : null);
  }

  // --- SCORM 1.2 ---------------------------------------------------------------

  function scorm12() {
    var api = findApi("API");
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
      saved: get("cmi.suspend_data"),
      save: function (snapshot, location) {
        if (finished) return;
        if (location) set("cmi.core.lesson_location", location);
        set("cmi.suspend_data", fitSnapshot(snapshot, 4096));
        api.LMSCommit("");
      },
      slideViewed: function () {},
      complete: function () {
        if (finished || completed) return;
        completed = true;
        set("cmi.core.lesson_status", "completed");
        api.LMSCommit("");
      },
      finish: function (snapshot) {
        if (finished) return;
        finished = true;
        if (snapshot) set("cmi.suspend_data", fitSnapshot(snapshot, 4096));
        set("cmi.core.exit", completed ? "" : "suspend");
        set("cmi.core.session_time", scorm12Duration(Date.now() - startedAt));
        api.LMSCommit("");
        api.LMSFinish("");
      }
    };
  }

  // --- SCORM 2004 ----------------------------------------------------------------

  function scorm2004() {
    var api = findApi("API_1484_11");
    if (!api || String(api.Initialize("")) !== "true") return null;
    var startedAt = Date.now();
    var finished = false;
    var get = function (key) {
      return String(api.GetValue(key) || "");
    };
    var set = function (key, value) {
      api.SetValue(key, String(value));
    };
    var completed = get("cmi.completion_status") === "completed";
    if (!completed) set("cmi.completion_status", "incomplete");
    api.Commit("");

    return {
      saved: get("cmi.entry") === "ab-initio" ? "" : get("cmi.suspend_data"),
      save: function (snapshot, location) {
        if (finished) return;
        if (location) set("cmi.location", location.slice(0, 1000));
        set("cmi.suspend_data", fitSnapshot(snapshot, 64000));
        api.Commit("");
      },
      slideViewed: function () {},
      complete: function () {
        if (finished || completed) return;
        completed = true;
        set("cmi.completion_status", "completed");
        api.Commit("");
      },
      finish: function (snapshot) {
        if (finished) return;
        finished = true;
        if (snapshot) set("cmi.suspend_data", fitSnapshot(snapshot, 64000));
        set("cmi.exit", completed ? "normal" : "suspend");
        set("cmi.session_time", isoDuration(Date.now() - startedAt));
        api.Commit("");
        api.Terminate("");
      }
    };
  }

  // --- xAPI plumbing (shared by xAPI and cmi5) --------------------------------------

  function uuid() {
    if (window.crypto && crypto.randomUUID) return crypto.randomUUID();
    var b = new Uint8Array(16);
    (window.crypto || { getRandomValues: function (a) {
      for (var i = 0; i < a.length; i++) a[i] = Math.floor(Math.random() * 256);
      return a;
    } }).getRandomValues(b);
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    var hex = Array.prototype.map.call(b, function (x) { return pad(x.toString(16), 2); }).join("");
    return hex.slice(0, 8) + "-" + hex.slice(8, 12) + "-" + hex.slice(12, 16) + "-" + hex.slice(16, 20) + "-" + hex.slice(20);
  }

  function query() {
    var out = {};
    var search = window.location.search.replace(/^\?/, "");
    if (!search) return out;
    search.split("&").forEach(function (pair) {
      var i = pair.indexOf("=");
      var key = decodeURIComponent((i < 0 ? pair : pair.slice(0, i)).replace(/\+/g, " "));
      out[key] = i < 0 ? "" : decodeURIComponent(pair.slice(i + 1).replace(/\+/g, " "));
    });
    return out;
  }

  /** Older (Tin Can 0.95) actors wrap values in arrays; unwrap them. */
  function normalizeActor(actor) {
    var out = {};
    Object.keys(actor).forEach(function (k) {
      out[k] = Array.isArray(actor[k]) ? actor[k][0] : actor[k];
    });
    if (out.account && Array.isArray(out.account)) out.account = out.account[0];
    if (!out.objectType) out.objectType = "Agent";
    return out;
  }

  function Lrs(endpoint, auth) {
    this.endpoint = endpoint.replace(/\/?$/, "/");
    this.auth = auth;
    this.chain = Promise.resolve();
  }

  Lrs.prototype.request = function (method, path, body, keepalive) {
    var headers = { "X-Experience-API-Version": "1.0.3" };
    if (this.auth) headers.Authorization = this.auth;
    if (body !== undefined) headers["Content-Type"] = "application/json";
    return fetch(this.endpoint + path, {
      method: method,
      headers: headers,
      body: body,
      keepalive: !!keepalive
    });
  };

  /** Send statements in order; a failure is logged, not fatal. */
  Lrs.prototype.send = function (statement, keepalive) {
    var self = this;
    this.chain = this.chain.then(function () {
      return self.request("POST", "statements", JSON.stringify(statement), keepalive).then(function (r) {
        if (!r.ok) warn("the LRS rejected a statement (" + r.status + ")");
      });
    }).catch(function (e) {
      warn("could not send a statement: " + e);
    });
    return this.chain;
  };

  Lrs.prototype.stateUrl = function (activityId, actor, registration, stateId) {
    var q = "activityId=" + encodeURIComponent(activityId) +
      "&agent=" + encodeURIComponent(JSON.stringify(actor)) +
      "&stateId=" + encodeURIComponent(stateId);
    if (registration) q += "&registration=" + encodeURIComponent(registration);
    return "activities/state?" + q;
  };

  Lrs.prototype.getState = function (activityId, actor, registration, stateId) {
    return this.request("GET", this.stateUrl(activityId, actor, registration, stateId)).then(function (r) {
      return r.ok ? r.text() : "";
    }).catch(function () {
      return "";
    });
  };

  Lrs.prototype.putState = function (activityId, actor, registration, stateId, body, keepalive) {
    var self = this;
    this.chain = this.chain.then(function () {
      return self.request("PUT", self.stateUrl(activityId, actor, registration, stateId), body, keepalive);
    }).catch(function (e) {
      warn("could not save progress: " + e);
    });
    return this.chain;
  };

  var VERBS = {
    initialized: "http://adlnet.gov/expapi/verbs/initialized",
    experienced: "http://adlnet.gov/expapi/verbs/experienced",
    completed: "http://adlnet.gov/expapi/verbs/completed",
    terminated: "http://adlnet.gov/expapi/verbs/terminated"
  };
  var STATE_ID = "https://w3id.org/storycraft/state/resume";

  /**
   * The shared adapter. `options.context(verb)` builds the statement context
   * and `options.sendCompleted` decides whether "completed" may be sent.
   */
  function xapiAdapter(lrs, actor, activityId, registration, info, options) {
    var startedAt = Date.now();
    var finished = false;
    var completed = false;

    function statement(verb, object, result) {
      var s = {
        id: uuid(),
        timestamp: new Date().toISOString(),
        actor: actor,
        verb: { id: VERBS[verb], display: { "en-US": verb } },
        object: object || { objectType: "Activity", id: activityId },
        context: options.context(verb, object)
      };
      if (result) s.result = result;
      return s;
    }

    return lrs.getState(activityId, actor, registration, STATE_ID).then(function (saved) {
      lrs.send(statement("initialized"));
      return {
        saved: saved,
        save: function (snapshot) {
          if (finished) return;
          lrs.putState(activityId, actor, registration, STATE_ID, snapshot);
        },
        slideViewed: function (id, name) {
          if (finished || !options.slideStatements) return;
          lrs.send(statement("experienced", {
            objectType: "Activity",
            id: activityId + "/slides/" + encodeURIComponent(id),
            definition: { name: { "en-US": name }, type: "http://adlnet.gov/expapi/activities/module" }
          }));
        },
        complete: function () {
          if (finished || completed || !options.sendCompleted) return;
          completed = true;
          lrs.send(statement("completed", null, { completion: true, duration: isoDuration(Date.now() - startedAt) }));
        },
        finish: function (snapshot) {
          if (finished) return;
          finished = true;
          if (snapshot) lrs.putState(activityId, actor, registration, STATE_ID, snapshot, true);
          return lrs.send(statement("terminated", null, { duration: isoDuration(Date.now() - startedAt) }), true)
            .then(function () {
              if (options.afterFinish) options.afterFinish();
            });
        }
      };
    });
  }

  // --- xAPI (Tin Can launch) ------------------------------------------------------

  function xapi(info) {
    var q = query();
    if (!q.endpoint || !q.actor) return Promise.resolve(null);
    var actor;
    try {
      actor = normalizeActor(JSON.parse(q.actor));
    } catch (e) {
      warn("the launch link has an invalid actor");
      return Promise.resolve(null);
    }
    var activityId = q.activity_id || info.activityId;
    var registration = q.registration || "";
    var lrs = new Lrs(q.endpoint, q.auth || "");
    return xapiAdapter(lrs, actor, activityId, registration, info, {
      slideStatements: true,
      sendCompleted: true,
      context: function (verb, object) {
        var context = {};
        if (registration) context.registration = registration;
        if (object && object.id !== activityId) {
          context.contextActivities = { parent: [{ objectType: "Activity", id: activityId }] };
        }
        return context;
      }
    });
  }

  // --- cmi5 -----------------------------------------------------------------------

  var CMI5_CATEGORY = { id: "https://w3id.org/xapi/cmi5/context/categories/cmi5" };
  var MOVEON_CATEGORY = { id: "https://w3id.org/xapi/cmi5/context/categories/moveon" };

  function cmi5() {
    var q = query();
    if (!q.endpoint || !q.fetch || !q.actor || !q.registration || !q.activityId) return Promise.resolve(null);
    var actor = normalizeActor(JSON.parse(q.actor));
    // The fetch URL hands out the session's auth token exactly once.
    return fetch(q.fetch, { method: "POST" }).then(function (r) {
      return r.json();
    }).then(function (body) {
      if (!body || !body["auth-token"]) throw new Error(body && body["error-text"] || "no auth token");
      var lrs = new Lrs(q.endpoint, "Basic " + body["auth-token"]);
      return lrs.getState(q.activityId, actor, q.registration, "LMS.LaunchData").then(function (text) {
        var launch = text ? JSON.parse(text) : {};
        var template = launch.contextTemplate || {};
        var normal = (launch.launchMode || "Normal") === "Normal";
        return xapiAdapter(lrs, actor, q.activityId, q.registration, {}, {
          slideStatements: false,
          sendCompleted: normal,
          context: function (verb) {
            var context = JSON.parse(JSON.stringify(template));
            context.registration = q.registration;
            context.contextActivities = context.contextActivities || {};
            var category = (context.contextActivities.category || []).concat([CMI5_CATEGORY]);
            if (verb === "completed") category.push(MOVEON_CATEGORY);
            context.contextActivities.category = category;
            return context;
          },
          afterFinish: function () {
            if (launch.returnURL) window.location.href = launch.returnURL;
          }
        });
      });
    }).catch(function (e) {
      warn("cmi5 launch failed: " + (e && e.message ? e.message : e));
      return null;
    });
  }

  window.StoryCraftLms = {
    connect: function (target, info) {
      switch (target) {
        case "scorm12":
          return Promise.resolve(scorm12());
        case "scorm2004":
          return Promise.resolve(scorm2004());
        case "xapi":
          return xapi(info);
        case "cmi5":
          return cmi5();
        default:
          return Promise.resolve(null);
      }
    },
    // Exposed for tests.
    isoDuration: isoDuration
  };
})();
