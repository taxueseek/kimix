#!/usr/bin/env node
/**
 * kimix npm postinstall — 从 GitHub Releases 下载当前平台的 kimix 二进制。
 *
 * 行为：
 *  1. 解析 process.platform / process.arch → target triple（与 install.sh 一致）
 *  2. 下载 https://github.com/taxueseek/kimix/releases/download/v<ver>/kimix-<ver>-<triple>.tar.gz
 *  3. 下载同 release 的 SHA256SUMS，校验后再解压
 *  4. 解压出的二进制落到 <pkg>/bin/<triple>/kimix[.exe]
 *
 * 环境变量：
 *  KIMIX_NPM_VERSION  覆盖安装版本（默认取 package.json 的 version，如 "0.1.16"）
 *  KIMIX_DOWNLOAD_BASE 覆盖下载基址（默认 https://github.com/taxueseek/kimix/releases/download）
 *  KIMIX_NPM_SKIP     =1 时跳过下载（调试用）
 *
 * 失败时不抛异常中断 npm 安装，而是打印回退指引（curl install.sh / install.ps1）。
 */

"use strict";

const crypto = require("crypto");
const fs = require("fs");
const os = require("os");
const path = require("path");
const { execFileSync } = require("child_process");

const REPO = "taxueseek/kimix";
const PKG_DIR = path.join(__dirname, "..");
const PKG = require(path.join(PKG_DIR, "package.json"));

const VERSION = (process.env.KIMIX_NPM_VERSION || PKG.version).replace(/^v/, "");
const DOWNLOAD_BASE =
  process.env.KIMIX_DOWNLOAD_BASE ||
  `https://github.com/${REPO}/releases/download`;
const BIN_DIR = path.join(PKG_DIR, "bin");

function platformInfo() {
  const plat = process.platform;
  const arch = process.arch;
  if (plat === "darwin") {
    if (arch === "arm64") return { os: "macos", triple: "aarch64-apple-darwin", ext: "", archiveExt: "tar.gz" };
    if (arch === "x64") return { os: "macos", triple: "x86_64-apple-darwin", ext: "", archiveExt: "tar.gz" };
  }
  if (plat === "linux") {
    if (arch === "arm64") return { os: "linux", triple: "aarch64-unknown-linux-gnu", ext: "", archiveExt: "tar.gz" };
    if (arch === "x64") return { os: "linux", triple: "x86_64-unknown-linux-gnu", ext: "", archiveExt: "tar.gz" };
  }
  if (plat === "win32" && arch === "x64") {
    return { os: "windows", triple: "x86_64-pc-windows-msvc", ext: ".exe", archiveExt: "zip" };
  }
  return null;
}

function download(url, dest) {
  // 优先用系统 curl（支持重定向与 TLS），回退到 https 模块。
  return new Promise((resolve, reject) => {
    try {
      execFileSync("curl", ["-fsSL", "-o", dest, url], { stdio: "pipe" });
      resolve();
      return;
    } catch (_) {
      /* curl 不可用或失败，走 https 模块 */
    }
    const https = require("https");
    const out = fs.createWriteStream(dest);
    https
      .get(url, { headers: { "User-Agent": "kimix-npm-install" } }, (res) => {
        if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
          out.close();
          fs.unlinkSync(dest);
          download(res.headers.location, dest).then(resolve, reject);
          return;
        }
        if (res.statusCode !== 200) {
          out.close();
          fs.unlinkSync(dest);
          reject(new Error(`HTTP ${res.statusCode} for ${url}`));
          return;
        }
        res.pipe(out);
        out.on("finish", () => {
          out.close(resolve);
        });
      })
      .on("error", (err) => {
        out.close();
        fs.unlinkSync(dest);
        reject(err);
      });
  });
}

function sha256Of(file) {
  const buf = fs.readFileSync(file);
  return crypto.createHash("sha256").update(buf).digest("hex");
}

function extract(archivePath, destDir) {
  fs.mkdirSync(destDir, { recursive: true });
  // 系统 tar：macOS/Linux 自带；Windows 10+ 的 bsdtar 同样支持解 .zip。
  execFileSync("tar", ["-xf", archivePath, "-C", destDir], { stdio: "pipe" });
}

async function main() {
  if (process.env.KIMIX_NPM_SKIP === "1") {
    console.log("kimix: KIMIX_NPM_SKIP=1, 跳过二进制下载");
    return;
  }
  const info = platformInfo();
  if (!info) {
    console.warn(
      `kimix: 不支持当前平台 ${process.platform}/${process.arch}，` +
        `请改用官方安装脚本（https://github.com/${REPO}）`
    );
    return;
  }

  const asset = `kimix-${VERSION}-${info.triple}.${info.archiveExt}`;
  const assetUrl = `${DOWNLOAD_BASE}/v${VERSION}/${asset}`;
  const sumsUrl = `${DOWNLOAD_BASE}/v${VERSION}/SHA256SUMS`;
  const destDir = path.join(BIN_DIR, info.triple);
  const binPath = path.join(destDir, `kimix${info.ext}`);

  if (fs.existsSync(binPath)) {
    // 已安装过（npm 重装 / 版本未变），直接复用。
    return;
  }

  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "kimix-npm-"));
  try {
    const archivePath = path.join(tmp, asset);
    const sumsPath = path.join(tmp, "SHA256SUMS");
    console.log(`kimix: 下载 v${VERSION}（${info.triple}）…`);
    await download(assetUrl, archivePath);
    await download(sumsUrl, sumsPath);

    const expected = fs
      .readFileSync(sumsPath, "utf8")
      .split("\n")
      .map((l) => l.trim().split(/\s+/))
      .find((parts) => parts.length >= 2 && parts[1].replace(/^\*/, "") === asset);
    if (!expected) {
      throw new Error(`SHA256SUMS 中没有 ${asset} 条目，拒绝安装未校验二进制`);
    }
    const actual = sha256Of(archivePath);
    if (actual !== expected[0]) {
      throw new Error(`SHA256 校验失败：期望 ${expected[0]}，实际 ${actual}`);
    }
    console.log("kimix: 校验通过，解压中…");
    extract(archivePath, destDir);
    if (!fs.existsSync(binPath)) {
      throw new Error(`解压结果中缺少二进制 ${binPath}`);
    }
    if (info.os !== "windows") {
      fs.chmodSync(binPath, 0o755);
    }
    console.log(`kimix v${VERSION} 已就绪：${binPath}`);
  } catch (err) {
    fs.rmSync(tmp, { recursive: true, force: true });
    fs.rmSync(destDir, { recursive: true, force: true });
    console.warn(
      `kimix: 自动下载失败（${err.message}）。` +
        `请用官方脚本安装：curl -fsSL https://raw.githubusercontent.com/${REPO}/main/install.sh | bash` +
        `（Windows: irm https://raw.githubusercontent.com/${REPO}/main/install.ps1 | iex）`
    );
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
}

main();
