#!/usr/bin/env node
/**
 * prepublishOnly 门禁：npm 包版本必须与 GitHub Releases 的 tag 对齐，
 * 否则 `npx kimix` 会尝试下载一个不存在的 release 资产。
 *
 * 校验规则：
 *  - package.json version 必须是 X.Y.Z（不带 v 前缀）
 *  - 同名 tag v<version> 必须已发布到 GitHub
 */

"use strict";

const https = require("https");
const path = require("path");
const PKG = require(path.join(__dirname, "..", "package.json"));

const version = PKG.version;
if (!/^\d+\.\d+\.\d+$/.test(version)) {
  console.error(`kimix: 版本号必须是 X.Y.Z（当前 ${version}），npm 包无法映射到 GitHub release`);
  process.exit(1);
}

const tag = `v${version}`;
const url = `https://api.github.com/repos/taxueseek/kimix/releases/tags/${tag}`;

https
  .get(url, { headers: { "User-Agent": "kimix-npm-publish" } }, (res) => {
    if (res.statusCode === 200) {
      console.log(`kimix: 校验通过 — GitHub release ${tag} 已存在`);
      process.exit(0);
    }
    if (res.statusCode === 404) {
      console.error(`kimix: GitHub release ${tag} 不存在。请先发布 release，再同步 npm 版本。`);
      process.exit(1);
    }
    // 403（限流）等其它情况：放行但给出警告，避免误伤发布流程。
    console.warn(`kimix: 无法确认 release ${tag}（HTTP ${res.statusCode}），跳过校验`);
    process.exit(0);
  })
  .on("error", (err) => {
    console.warn(`kimix: GitHub API 不可达（${err.message}），跳过校验`);
    process.exit(0);
  });
