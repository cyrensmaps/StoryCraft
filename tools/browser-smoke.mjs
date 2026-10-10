// End-to-end smoke test for a published SAMPLE course (`storycraft-cli sample`).
//
// Drives a real Chromium-based browser over the DevTools protocol: clicks
// through the sample course, checks timing, variables, layers and states, and
// verifies SCORM 1.2 calls against a fake LMS API.
//
//   node tools/browser-smoke.mjs <published-scorm-folder> [screenshot-dir]
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
const browserPath = process.env.BROWSER ?? "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe";

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let failures = 0;
function check(cond, what) {
  console.log(`${cond ? "ok  " : "FAIL"} ${what}`);
  if (!cond) failures++;
}

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

// A minimal SCORM 1.2 API that records every call.
const fakeLms = `
  window.__lms = { values: { "cmi.core.lesson_status": "not attempted", "cmi.suspend_data": "" }, calls: [] };
  window.API = {
    LMSInitialize: () => { __lms.calls.push("init"); return "true"; },
    LMSFinish: () => { __lms.calls.push("finish"); return "true"; },
    LMSGetValue: (k) => __lms.values[k] ?? "",
    LMSSetValue: (k, v) => { __lms.values[k] = v; __lms.calls.push("set " + k); return "true"; },
    LMSCommit: () => { __lms.calls.push("commit"); return "true"; },
    LMSGetLastError: () => "0", LMSGetErrorString: () => "", LMSGetDiagnostic: () => "",
  };`;

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
  await send("Page.addScriptToEvaluateOnNewDocument", { source: fakeLms });
  await send("Page.navigate", { url: pathToFileURL(join(site, "index.html")).href });
  await sleep(1500);

  await screenshot("1-welcome");
  check((await texts()).includes("Welcome to StoryCraft"), "welcome slide renders");
  check((await texts()).includes("An open-source e-learning authoring tool"), "subtitle appears after 500 ms");
  check(await evaluate(`__lms.values["cmi.core.lesson_status"] === "incomplete"`), "LMS status set to incomplete");

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
  await sleep(100);
  check((await position()) === "Slide 3 of 3", "keyboard activation of Next works");
  check(await evaluate(`__lms.values["cmi.core.lesson_location"] !== undefined`), "bookmark saved to LMS");
  await sleep(2600);
  await screenshot("4-done");
  check((await texts()).includes("You clicked 3 times."), "timed summary text appears");
  check(await evaluate(`__lms.values["cmi.core.lesson_status"] === "completed"`), "course reported completed");
  const suspend = await evaluate(`__lms.values["cmi.suspend_data"]`);
  check(suspend.length > 0 && suspend.length <= 4096 && JSON.parse(suspend).slide, "suspend data is valid JSON within limits");

  // Resume: reload with the saved suspend data and expect slide 3.
  await send("Page.addScriptToEvaluateOnNewDocument", {
    source: `window.__lms.values["cmi.suspend_data"] = ${JSON.stringify(suspend)};`,
  });
  await send("Page.navigate", { url: pathToFileURL(join(site, "index.html")).href });
  await sleep(1000);
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
