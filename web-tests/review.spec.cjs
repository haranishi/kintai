// SPDX-FileCopyrightText: 2026 Ikki Nishihara and the kintai contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
const { test, expect } = require("@playwright/test");
const { spawnSync } = require("node:child_process");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { pathToFileURL } = require("node:url");
const binary = process.env.KINTAI_BIN || path.resolve("target/debug/kintai");
let directory;
let full;
let empty;
let scoped;
let anomalies;

function employeePolicy(count) {
  const source = fs.readFileSync("tests/fixtures/synthetic/policy.toml", "utf8");
  const employee = source.split("\n# E001")[1].split("\n# E002")[0];
  const base = source.split("\n# E001")[0];
  return base + Array.from({ length:count }, (_, index) => {
    const id = "E" + String(index + 1).padStart(3,"0");
    return "\n# " + id + employee.replaceAll("E001", id);
  }).join("");
}

function generate(name, extra = [], punches = "tests/fixtures/synthetic/punches.csv", policy = "tests/fixtures/synthetic/policy.toml") {
  const out = path.join(directory, name);
  const result = spawnSync(binary, ["review", "--policy", policy, "--punches", punches, "--month", "2026-10", "--out", out, ...extra], { encoding: "utf8" });
  expect([0, 4]).toContain(result.status);
  expect(result.stderr).toBe("");
  return pathToFileURL(out).href;
}
test.beforeAll(() => {
  directory = fs.mkdtempSync(path.join(os.tmpdir(), "kintai-browser-synthetic-"));
  full = generate("full.html");
  scoped = generate("scoped.html", ["--employee", "E001"]);
  const noPunches = path.join(directory, "empty.csv");
  fs.writeFileSync(noPunches, "employee_id,timestamp,kind\n");
  empty = generate("empty.html", [], noPunches);
  const anomalyPolicy = path.join(directory, "anomalies.toml");
  const anomalyPunches = path.join(directory, "anomalies.csv");
  fs.writeFileSync(anomalyPolicy, employeePolicy(25));
  const patterns = [
    [["09:00", "in"], ["09:01", "in"], ["17:00", "out"]],
    [["17:00", "out"]],
    [["09:00", "break_start"]],
    [["09:00", "break_end"]],
    [["09:00", "in"], ["12:00", "break_start"], ["17:00", "out"]],
    [["09:00", "in"]],
    [["09:00", "in"], ["09:00", "out"]],
    [["09:00", "in"], ["2026-10-06T09:00", "out"]],
  ];
  const rows = patterns.flatMap((pattern, index) => pattern.map(([time, kind]) =>
    `E${String(index + 1).padStart(3,"0")},${time.includes("T") ? time : "2026-10-05T" + time},${kind}`));
  fs.writeFileSync(anomalyPunches, "employee_id,timestamp,kind\n" + rows.join("\n") + "\nE025,2026-10-05T17:00,out\n");
  anomalies = generate("anomalies.html", [], anomalyPunches, anomalyPolicy);
});
test.afterAll(() => fs.rmSync(directory, { recursive:true, force:true }));

// AC-14.4: actual calculations, partial refusal and details, across device widths.
for (const width of [390, 768, 1440]) {
  test(`monthly verification at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height:1000 });
    const errors = [];
    const requests = [];
    page.on("pageerror", (error) => errors.push(error.message));
    page.on("request", (request) => { if (/^https?:/.test(request.url())) requests.push(request.url()); });
    await page.goto(full);
    await expect(page.getByRole("heading", { name:"月次の確認", exact:true })).toBeVisible();
    await expect(page.locator("#totals-scope")).toHaveText("集計できた 4人 / 全5人 の合計");
    await expect(page.locator("#status-title")).toHaveText("2人の記録を確認してください");
    const refused = page.locator('[data-employee="E005"]');
    await expect(refused.locator("summary")).toContainText("集計不可");
    await expect(refused.locator("summary")).not.toContainText("0時間");
    await refused.locator("summary").click();
    await expect(refused.locator(".employee-detail")).toContainText("勤務の帰属が曖昧");
    const employee = page.locator('[data-employee="E001"]');
    await expect(employee.locator("summary")).toContainText("152時間32分");
    await employee.locator("summary").focus();
    await page.keyboard.press("Enter");
    await expect(employee).toHaveAttribute("open", "");
    await expect(employee.locator("dd").first()).toHaveText("21日");
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    expect(errors).toEqual([]);
    expect(requests).toEqual([]);
  });
}

// AC-14.5: filters, recovery, keyboard, original download, empty state and dark appearance.
test("filters recover and JSON export preserves the full original scope", async ({ page }) => {
  await page.goto(full);
  await page.getByRole("button", { name:"確認が必要", exact:true }).click();
  await expect(page.locator(".employee-row")).toHaveCount(2);
  await page.getByLabel("従業員IDで検索").fill("no-match");
  await expect(page.getByRole("heading", { name:"該当する従業員はいません" })).toBeVisible();
  await page.getByRole("button", { name:"条件をリセット" }).click();
  await expect(page.getByLabel("従業員IDで検索")).toBeFocused();
  await expect(page.locator(".employee-row")).toHaveCount(5);
  await page.getByLabel("従業員IDで検索").fill("e001");
  await expect(page.locator(".employee-row")).toHaveCount(1);
  const original = await page.locator("#report-data").textContent();
  const downloadPromise = page.waitForEvent("download");
  await page.getByRole("button", { name:"JSONを書き出す" }).click();
  const download = await downloadPromise;
  expect(download.suggestedFilename()).toBe("kintai-2026-10.json");
  expect(JSON.parse(fs.readFileSync(await download.path(), "utf8"))).toEqual(JSON.parse(original));
});

test("missing punches and explicitly scoped reports are distinguishable", async ({ page }) => {
  await page.goto(empty);
  await expect(page.locator("#status-title")).toHaveText("対象期間の打刻がありません");
  await expect(page.locator("#metrics")).toContainText(/0時間\s*0分/);
  await page.goto(scoped);
  await expect(page.locator("#totals-scope")).toHaveText("集計できた 1人 / 全1人 の合計");
  await expect(page.locator("#report-data")).not.toContainText("E005");
});

test("dark mobile view keeps controls visible with named landmarks and keyboard access", async ({ page }) => {
  await page.setViewportSize({ width:390, height:844 });
  await page.emulateMedia({ colorScheme:"dark", reducedMotion:"reduce" });
  await page.goto(full);
  await page.keyboard.press("Tab");
  await expect(page.getByRole("link", { name:"本文へ移動" })).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(page.locator("main")).toBeFocused();
  await expect(page.getByRole("navigation", { name:"ページ内" })).toBeVisible();
  await expect(page.getByRole("button", { name:"JSONを書き出す" })).toBeVisible();
  const buttons = await page.locator("button:visible").evaluateAll((nodes) => nodes.map((node) => ({ name:node.textContent, height:node.getBoundingClientRect().height })));
  expect(buttons.filter((button) => button.height < 44)).toEqual([]);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
});

test("pagination preserves all synthetic employees", async ({ page }) => {
  const manyPolicy = path.join(directory, "many.toml");
  fs.writeFileSync(manyPolicy, employeePolicy(25));
  const many = generate("many.html", [], path.join(directory,"empty.csv"), manyPolicy);
  await page.goto(many);
  await expect(page.locator(".employee-row")).toHaveCount(20);
  await page.getByRole("button", { name:"次へ", exact:true }).click();
  await expect(page.locator(".employee-row")).toHaveCount(5);
  await expect(page.locator("#page-label")).toHaveText("2 / 2ページ");
  await page.getByRole("button", { name:"前へ", exact:true }).click();
  await expect(page.locator(".employee-row")).toHaveCount(20);
});

// AC-14.6: use real CSV anomalies, never injected browser-side report data.
test("all real punch anomalies explain what needs correction", async ({ page }) => {
  await page.goto(anomalies);
  const document = JSON.parse(await page.locator("#report-data").textContent());
  const cases = [
    ["DuplicateIn", "出勤が重複しています"],
    ["OutWithoutIn", "対応する出勤がありません"],
    ["BreakStartWithoutIn", "勤務中でないときに休憩開始"],
    ["BreakEndWithoutStart", "対応する休憩開始がありません"],
    ["OutDuringBreak", "休憩を終了する前に退勤"],
    ["OpenShift", "退勤の記録がありません"],
    ["SameTimestamp", "同じ時刻に複数の打刻"],
    ["LongShift", "勤務が24時間以上"],
  ];
  for (const [index, [code, explanation]] of cases.entries()) {
    const id = "E" + String(index + 1).padStart(3,"0");
    expect(document.employees[index].refused.anomalies.map((item) => item.kind)).toContain(code);
    const row = page.locator(`[data-employee="${id}"]`);
    await row.locator("summary").click();
    await expect(row.locator(".employee-detail")).toContainText(explanation);
    await expect(row.locator(".employee-detail")).not.toContainText("記録の確認が必要です（");
    await expect(row.locator("summary")).not.toContainText("0時間");
  }
});

test("checklist opens a later-page employee despite active search and status filters", async ({ page }) => {
  await page.setViewportSize({ width:390, height:844 });
  await page.goto(anomalies);
  await page.getByLabel("従業員IDで検索").fill("E001");
  await page.getByRole("button", { name:"確認が必要", exact:true }).click();
  const action = page.getByRole("button", { name:"E025の記録を確認", exact:true });
  await action.focus();
  await page.keyboard.press("Enter");
  const target = page.locator('[data-employee="E025"]');
  await expect(target).toHaveAttribute("open", "");
  await expect(target.locator("summary")).toBeFocused();
  await expect(page.getByLabel("従業員IDで検索")).toHaveValue("");
  await expect(page.getByRole("button", { name:"すべて", exact:true })).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator("#page-label")).toHaveText("2 / 2ページ");
  await expect(target.locator(".employee-detail")).toContainText("対応する出勤がありません");
  await expect(target.locator("summary")).not.toContainText("0時間");
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
});

test("text and status colors retain WCAG AA contrast in both appearances", async ({ page }) => {
  for (const colorScheme of ["light", "dark"]) {
    await page.emulateMedia({ colorScheme });
    await page.goto(full);
    const failures = await page.locator("button, input, .badge, .muted, .footnote, .eyebrow, .row-label, .metric-label").evaluateAll((nodes) => {
      const rgb = (value) => value.match(/[\d.]+/g).map(Number);
      const luminance = (channels) => channels.slice(0,3).map((value) => {
        const channel = value / 255;
        return channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4;
      }).reduce((total, value, index) => total + value * [0.2126,0.7152,0.0722][index],0);
      return nodes.filter((node) => node.getClientRects().length).map((node) => {
        let ancestor = node;
        let background;
        while (ancestor) {
          const color = rgb(getComputedStyle(ancestor).backgroundColor);
          if (color.length < 4 || color[3] > 0) { background=color; break; }
          ancestor = ancestor.parentElement;
        }
        const foreground = luminance(rgb(getComputedStyle(node).color));
        const backdrop = luminance(background || [255,255,255]);
        return { text:node.textContent.trim().slice(0,40), ratio:(Math.max(foreground,backdrop)+0.05)/(Math.min(foreground,backdrop)+0.05) };
      }).filter((item) => item.ratio < 4.5);
    });
    expect(failures, colorScheme).toEqual([]);
  }
});
