// SPDX-FileCopyrightText: 2026 Ikki Nishihara and the kintai contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
const { defineConfig } = require("@playwright/test");
module.exports = defineConfig({
  testDir: "./web-tests",
  timeout: 30_000,
  retries: 0,
  fullyParallel: true,
  forbidOnly: Boolean(process.env.CI),
  reporter: "list",
  use: { browserName: "chromium", trace: "retain-on-failure", screenshot: "only-on-failure" },
});
