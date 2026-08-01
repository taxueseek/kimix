# kimix Homebrew formula 模板
#
# 用途：把本文件放到自建 tap 仓库（例如 taxueseek/homebrew-kimix 的
# Formula/ 目录）后，用户即可通过以下方式安装：
#
#   brew tap taxueseek/homebrew-kimix
#   brew install kimix
#
# 发布流程（每发一版更新一次）：
#   1. 从 GitHub release 的 SHA256SUMS 资产中，把对应平台的哈希填入下方
#      sha256 字段（本模板以 macOS arm64 为例，其他平台用 on_arm / on_intel
#      分支分别指定）。
#   2. 校验：brew audit --strict --online kimix
#
# 说明：Homebrew 会直接解压 release 的 tar.gz 资产（内含单文件二进制
# `kimix`），无需编译，与官方 install.sh 使用同一份产物。

class Kimix < Formula
  desc "通用终端 AI 代理（unofficial Kimi Code CLI community build）"
  homepage "https://github.com/taxueseek/kimix"
  license "Apache-2.0"

  version "0.1.16"

  on_arm do
    url "https://github.com/taxueseek/kimix/releases/download/v0.1.16/kimix-0.1.16-aarch64-apple-darwin.tar.gz"
    sha256 "REPLACE_WITH_SHA256_FROM_SHA256SUMS" # macOS arm64
  end

  on_intel do
    url "https://github.com/taxueseek/kimix/releases/download/v0.1.16/kimix-0.1.16-x86_64-apple-darwin.tar.gz"
    sha256 "REPLACE_WITH_SHA256_FROM_SHA256SUMS" # macOS x86_64
  end

  def install
    bin.install "kimix"
  end

  test do
    assert_match "kimix", shell_output("#{bin}/kimix --version")
  end
end
