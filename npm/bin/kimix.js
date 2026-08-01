#!/usr/bin/env node
/**
 * kimix npm bin 包装器。
 *
 * npm 的 bin 入口必须是一个可执行脚本；真实二进制由 postinstall 下载到
 * <pkg>/bin/<triple>/kimix[.exe]，这里负责定位并转发所有参数与退出码。
 */

"use strict";

const fs = require("fs");
const path = require("path");
const { spawn } = require("child_process");

const PKG_DIR = path.join(__dirname, "..");
const BIN_DIR = path.join(PKG_DIR, "bin");

function binaryPath() {
  const plat = process.platform;
  const arch = process.arch;
  let triple;
  if (plat === "darwin") triple = arch === "arm64" ? "aarch64-apple-darwin" : "x86_64-apple-darwin";
  else if (plat === "linux") triple = arch === "arm64" ? "aarch64-unknown-linux-gnu" : "x86_64-unknown-linux-gnu";
  else if (plat === "win32") triple = "x86_64-pc-windows-msvc";
  else return null;
  const bin = path.join(BIN_DIR, triple, plat === "win32" ? "kimix.exe" : "kimix");
  return fs.existsSync(bin) ? bin : null;
}

const bin = binaryPath();
if (!bin) {
  console.error(
    "kimix: 未找到平台二进制。npm 安装时的自动下载可能失败，请改用官方脚本：\n" +
      "  macOS/Linux: curl -fsSL https://raw.githubusercontent.com/taxueseek/kimix/main/install.sh | bash\n" +
      "  Windows:     irm https://raw.githubusercontent.com/taxueseek/kimix/main/install.ps1 | iex"
  );
  process.exit(1);
}

const child = spawn(bin, process.argv.slice(2), { stdio: "inherit" });
child.on("exit", (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
    return;
  }
  process.exit(code === null ? 1 : code);
});
child.on("error", (err) => {
  console.error(`kimix: 启动失败：${err.message}`);
  process.exit(1);
});
