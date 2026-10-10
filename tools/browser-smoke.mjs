// End-to-end smoke test for a published SAMPLE course (`storycraft-cli sample`).
//
// Drives a real Chromium-based browser over the DevTools protocol: clicks
// through the sample course, checks timing, variables, layers and states, and
// verifies the LMS conversation for the package's format (read from
// storycraft/course.js) against a fake: a SCORM 1.2 or 2004 API object, or a
// fake LRS that answers the course's xAPI / cmi5 requests.
//
//   node tools/browser-smoke.mjs <published-folder> [screenshot-dir]
//
// The browser defaults to Microsoft Edge on Windows; set BROWSER to a Chrome
// or Chromium executable elsewhere. Requires Node 22+ (global WebSocket).

import { spawn } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync, existsSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const site = resolve(process.argv[2] ?? "");
const shots = process.argv[3] ? resolve(process.argv[3]) : null;
if (!existsSync(join(site, "index.html"))) {
  console.error("usage: node tools/browser-smoke.mjs <published-folder> [screenshot-dir]");
  process.exit(2);
}
const format = /STORYCRAFT_TARGET = "(\w+)"/.exec(readFileSync(join(site, "storycraft/course.js"), "utf8"))?.[1];
const browserPath = process.env.BROWSER ?? "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe";

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let failures = 0;
function check(cond, what) {
  console.log(`${cond ? "ok  " : "FAIL"} ${what}`);
  if (!cond) failures++;
}

// --- fake LMS profiles ---------------------------------------------------------

const fakeScorm12 = `
  window.__lms = { values: { "cmi.core.lesson_status": "not attempted", "cmi.suspend_data": "" }, calls: [] };
  window.API = {
    LMSInitialize: () => { __lms.calls.push("init"); return "true"; },
    LMSFinish: () => { __lms.calls.push("finish"); return "true"; },
    LMSGetValue: (k) => __lms.values[k] ?? "",
    LMSSetValue: (k, v) => { __lms.values[k] = v; return "true"; },
    LMSCommit: () => "true",
    LMSGetLastError: () => "0", LMSGetErrorString: () => "", LMSGetDiagnostic: () => "",
  };`;

const fakeScorm2004 = `
  window.__lms = { values: { "cmi.completion_status": "unknown", "cmi.entry": "ab-initio", "cmi.suspend_data": "" }, calls: [] };
  window.API_1484_11 = {
    Initialize: () => { __lms.calls.push("init"); return "true"; },
    Terminate: () => { __lms.calls.push("terminate"); return "true"; },
    GetValue: (k) => __lms.values[k] ?? "",
    SetValue: (k, v) => { __lms.values[k] = v; return "true"; },
    Commit: () => "true",
    GetLastError: () => "0", GetErrorString: () => "", GetDiagnostic: () => "",
  };`;

// Answers the course's fetch() calls as an LRS (and, for cmi5, the LMS's
// auth-token "fetch" URL) would.
const fakeLrs = `
  window.__lrs = { requests: [], statements: [], state: {}, fetched: 0 };
  const realFetch = window.fetch.bind(window);
  window.fetch = async (url, opts = {}) => {
    const u = new URL(url, location.href);
    const method = (opts.method || "GET").toUpperCase();
    __lrs.requests.push({ method, url: u.href, headers: opts.headers || {} });
    if (u.href.startsWith("https://lms.test/fetch")) {
      __lrs.fetched++;
      return new Response(JSON.stringify({ "auth-token": "dG9rZW4=" }), { status: 200 });
    }
    if (!u.href.startsWith("https://lrs.test/xapi/")) return realFetch(url, opts);
    const path = u.pathname.replace("/xapi/", "");
    if (path === "statements" && method === "POST") {
      const body = JSON.parse(opts.body);
      (Array.isArray(body) ? body : [body]).forEach((s) => __lrs.statements.push(s));
      return new Response("[]", { status: 200 });
    }
    if (path === "activities/state") {
      const key = u.searchParams.get("stateId");
      if (method === "PUT") { __lrs.state[key] = opts.body; return new Response(null, { status: 204 }); }
      return key in __lrs.state ? new Response(__lrs.state[key], { status: 200 }) : new Response("", { status: 404 });
    }
    return new Response("", { status: 400 });
  };`;

const RESUME_STATE = "https://w3id.org/storycraft/state/resume";
const SESSION = "https://w3id.org/xapi/cmi5/context/extensions/sessionid";
const CMI5 = "https://w3id.org/xapi/cmi5/context/categories/cmi5";
const MOVEON = "https://w3id.org/xapi/cmi5/context/categories/moveon";
const REGISTRATION = "8a2f0c5e-2a4b-4f5e-9d3c-1b2a3c4d5e6f";
const verbOf = (s) => s.verb.id.split("/").pop();

const statements = () => evaluate(`__lrs.statements`);

const profiles = {
  scorm12: {
    fake: fakeScorm12,
    started: () => evaluate(`__lms.values["cmi.core.lesson_status"] === "incomplete"`),
    bookmarked: () => evaluate(`!!__lms.values["cmi.core.lesson_location"]`),
    completed: () => evaluate(`__lms.values["cmi.core.lesson_status"] === "completed"`),
    saved: () => evaluate(`__lms.values["cmi.suspend_data"]`),
    limit: 4096,
    finished: () => evaluate(`__lms.calls.includes("finish") && __lms.values["cmi.core.exit"] === "" && /^\\d{4}:\\d\\d:\\d\\d/.test(__lms.values["cmi.core.session_time"])`),
    seed: (saved) => `__lms.values["cmi.suspend_data"] = ${JSON.stringify(saved)};`,
  },
  scorm2004: {
    fake: fakeScorm2004,
    started: () => evaluate(`__lms.values["cmi.completion_status"] === "incomplete"`),
    bookmarked: () => evaluate(`!!__lms.values["cmi.location"]`),
    completed: () => evaluate(`__lms.values["cmi.completion_status"] === "completed"`),
    saved: () => evaluate(`__lms.values["cmi.suspend_data"]`),
    limit: 64000,
    finished: () => evaluate(`__lms.calls.includes("terminate") && __lms.values["cmi.exit"] === "normal" && /^PT\\d+H\\d+M[\\d.]+S$/.test(__lms.values["cmi.session_time"])`),
    seed: (saved) => `__lms.values["cmi.entry"] = "resume"; __lms.values["cmi.suspend_data"] = ${JSON.stringify(saved)};`,
  },
  xapi: {
    fake: fakeLrs,
    query: "?" + new URLSearchParams({
      endpoint: "https://lrs.test/xapi/",
      auth: "Basic bGVhcm5lcjpzZWNyZXQ=",
      actor: JSON.stringify({ mbox: "mailto:learner@example.com", name: "Learner" }),
      registration: REGISTRATION,
    }),
    started: async () => {
      const s = await statements();
      return s.length > 0 && verbOf(s[0]) === "initialized";
    },
    bookmarked: () => evaluate(`!!__lrs.state[${JSON.stringify(RESUME_STATE)}]`),
    completed: async () => (await statements()).some((s) => verbOf(s) === "completed" && s.result?.completion === true),
    saved: () => evaluate(`__lrs.state[${JSON.stringify(RESUME_STATE)}]`),
    limit: Infinity,
    finished: async () => {
      const s = await statements();
      return verbOf(s[s.length - 1]) === "terminated" && /^PT/.test(s[s.length - 1].result?.duration);
    },
    extra: async () => {
      const s = await statements();
      check(s.filter((x) => verbOf(x) === "experienced").length >= 3, "a statement for each slide viewed");
      check(s.every((x) => x.actor.mbox === "mailto:learner@example.com" && x.context.registration === REGISTRATION),
        "statements carry the launch actor and registration");
      const auth = await evaluate(`__lrs.requests.filter(r => r.url.includes("/statements")).every(r => r.headers.Authorization === "Basic bGVhcm5lcjpzZWNyZXQ=" && r.headers["X-Experience-API-Version"] === "1.0.3")`);
      check(auth, "statements are sent with the launch credentials and xAPI version header");
    },
    seed: (saved) => `__lrs.state[${JSON.stringify(RESUME_STATE)}] = ${JSON.stringify(saved)};`,
  },
  cmi5: {
    fake: fakeLrs + `
      __lrs.state["LMS.LaunchData"] = JSON.stringify({
        contextTemplate: {
          contextActivities: { grouping: [{ id: "urn:test:au" }] },
          extensions: { "${SESSION}": "session-1" },
        },
        launchMode: "Normal",
        moveOn: "Completed",
        returnURL: location.href.split("#")[0] + "#returned",
      });`,
    query: "?" + new URLSearchParams({
      endpoint: "https://lrs.test/xapi/",
      fetch: "https://lms.test/fetch?session=1",
      registration: REGISTRATION,
      activityId: "urn:test:au",
      actor: JSON.stringify({ objectType: "Agent", account: { homePage: "https://lms.test", name: "learner" } }),
    }),
    started: async () => {
      const s = await statements();
      return (await evaluate(`__lrs.fetched`)) === 1 && s.length > 0 && verbOf(s[0]) === "initialized";
    },
    bookmarked: () => evaluate(`!!__lrs.state[${JSON.stringify(RESUME_STATE)}]`),
    completed: async () => (await statements()).some((s) =>
      verbOf(s) === "completed" && s.result?.completion === true && /^PT/.test(s.result.duration) &&
      s.context.contextActivities.category.some((c) => c.id === MOVEON)),
    saved: () => evaluate(`__lrs.state[${JSON.stringify(RESUME_STATE)}]`),
    limit: Infinity,
    finished: async () => {
      const s = await statements();
      return verbOf(s[s.length - 1]) === "terminated" && (await evaluate(`location.hash`)) === "#returned";
    },
    extra: async () => {
      const s = await statements();
      check(s.every((x) => x.object.id === "urn:test:au"), "every statement is about the launched AU");
      check(s.every((x) => x.context.registration === REGISTRATION && x.context.extensions[SESSION] === "session-1" &&
        x.context.contextActivities.category.some((c) => c.id === CMI5) &&
        x.context.contextActivities.grouping[0].id === "urn:test:au"), "statements follow the cmi5 context template");
      const auth = await evaluate(`__lrs.requests.filter(r => r.url.includes("lrs.test")).every(r => r.headers.Authorization === "Basic dG9rZW4=")`);
      check(auth, "the fetched auth token is used for every LRS request");
    },
    seed: (saved) => `__lrs.state[${JSON.stringify(RESUME_STATE)}] = ${JSON.stringify(saved)};`,
  },
};

const lms = profiles[format];
if (!lms) {
  console.error(`unsupported package format ${format}; publish the sample as scorm12, scorm2004, xapi or cmi5`);
  process.exit(2);
}
const url = pathToFileURL(join(site, "index.html")).href + (lms.query ?? "");

// --- browser plumbing ----------------------------------------------------------

const profile = mkdtempSync(join(tmpdir(), "storycraft-smoke-"));
const browser = spawn(browserPath, [
  "--headless=new", "--disable-gpu", "--no-first-run", "--no-default-browser-check",
  `--user-data-dir=${profile}`, "--remote-debugging-port=0", "--window-size=1280,800",
  // CI containers (e.g. Ubuntu 24.04 runners) cannot use Chrome's sandbox.
  ...(process.env.CI ? ["--no-sandbox"] : []),
  "about:blank",
], { stdio: "ignore" });

async function devtoolsPort() {
  const file = join(profile, "DevToolsActivePort");
  for (let i = 0; i < 100; i++) {
    if (existsSync(file)) {
      const [port] = readFileSync(file, "utf8").split("\n");
      if (port) return port;
    }
    await sleep(100);
  }
  throw new Error("browser did not start");
}

let ws;
let nextId = 1;
const pending = new Map();
function send(method, params = {}) {
  const id = nextId++;
  ws.send(JSON.stringify({ id, method, params }));
  return new Promise((res, rej) => pending.set(id, { res, rej }));
}
async function evaluate(expression) {
  const r = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  if (r.exceptionDetails) throw new Error(JSON.stringify(r.exceptionDetails));
  return r.result.value;
}
async function screenshot(name) {
  if (!shots) return;
  const { data } = await send("Page.captureScreenshot", { format: "png" });
  writeFileSync(join(shots, `${name}.png`), Buffer.from(data, "base64"));
}
async function clickText(label) {
  const box = await evaluate(`(() => {
    const el = [...document.querySelectorAll('.sc-obj')].find(e => e.textContent.trim() === ${JSON.stringify(label)});
    if (!el) return null;
    const r = el.getBoundingClientRect();
    return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
  })()`);
  if (!box) throw new Error(`no object labelled ${label}`);
  for (const type of ["mouseMoved", "mousePressed", "mouseReleased"]) {
    await send("Input.dispatchMouseEvent", { type, x: box.x, y: box.y, button: "left", clickCount: 1 });
  }
  await sleep(100);
}
const texts = () => evaluate(`[...document.querySelectorAll('.sc-obj')].map(e => e.textContent.trim()).filter(Boolean)`);
const position = () => evaluate(`document.getElementById('sc-pos').textContent`);

// --- the run -------------------------------------------------------------------

console.log(`format: ${format}`);
try {
  const port = await devtoolsPort();
  const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
  const page = targets.find((t) => t.type === "page");
  ws = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((r) => ws.addEventListener("open", r, { once: true }));
  ws.addEventListener("message", (e) => {
    const msg = JSON.parse(e.data);
    if (msg.id && pending.has(msg.id)) {
      const { res, rej } = pending.get(msg.id);
      pending.delete(msg.id);
      msg.error ? rej(new Error(msg.error.message)) : res(msg.result);
    }
  });
  await send("Page.enable");
  await send("Runtime.enable");
  await send("Page.addScriptToEvaluateOnNewDocument", { source: lms.fake });
  await send("Page.navigate", { url });
  await sleep(1500);

  await screenshot("1-welcome");
  check((await texts()).includes("Welcome to StoryCraft"), "welcome slide renders");
  check((await texts()).includes("An open-source e-learning authoring tool"), "subtitle appears after 500 ms");
  check(await lms.started(), "the LMS session starts");

  await clickText("Learn more");
  await screenshot("2-layer");
  check((await texts()).some((t) => t.startsWith("Slides hold layers")), "Learn more shows the Info layer");
  await clickText("Close");
  check(!(await texts()).some((t) => t.startsWith("Slides hold layers")), "Close hides the layer");

  await clickText("Start");
  check((await position()) === "Slide 2 of 3", "Start jumps to slide 2");
  check((await texts()).includes("Clicks: 0"), "variable text shows 0");
  const starVisible = () => evaluate(`!!document.querySelector('.sc-obj polygon')`);
  check(!(await starVisible()), "star hidden initially");
  for (let i = 0; i < 3; i++) await clickText("+1");
  check((await texts()).includes("Clicks: 3"), "three clicks counted");
  check(await starVisible(), "star revealed by variable condition");
  await screenshot("3-counter");

  // Keyboard activation goes through the button's click with detail 0.
  await evaluate(`[...document.querySelectorAll('button.sc-obj')].find(b => b.textContent.trim() === 'Next').click()`);
  await sleep(300);
  check((await position()) === "Slide 3 of 3", "keyboard activation of Next works");
  check(await lms.bookmarked(), "progress saved to the LMS");
  await sleep(2600);
  await screenshot("4-done");
  check((await texts()).includes("You clicked 3 times."), "timed summary text appears");
  check(await lms.completed(), "course reported completed");

  // Leaving the page ends the session.
  await evaluate(`window.dispatchEvent(new Event("pagehide"))`);
  await sleep(500);
  check(await lms.finished(), "the session is closed when the learner leaves");
  if (lms.extra) await lms.extra();

  const saved = await lms.saved();
  check(saved && saved.length <= lms.limit && JSON.parse(saved).slide, "saved progress is valid JSON within limits");

  // Resume: reload with the saved progress and expect slide 3.
  await send("Page.addScriptToEvaluateOnNewDocument", { source: lms.seed(saved) });
  await send("Page.navigate", { url });
  await sleep(1500);
  check((await position()) === "Slide 3 of 3", "course resumes at the bookmarked slide");
} catch (e) {
  console.error(e);
  failures++;
} finally {
  ws?.close();
  browser.kill();
  await sleep(300);
  try {
    rmSync(profile, { recursive: true, force: true });
  } catch { /* the browser may still hold files briefly */ }
}

console.log(failures ? `\n${failures} check(s) failed` : "\nall checks passed");
process.exit(failures ? 1 : 0);
