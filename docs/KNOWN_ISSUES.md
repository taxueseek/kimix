# 已知问题 / Known Issues

## Linux 二进制体积较大

Linux 平台的二进制（尤其是 aarch64-unknown-linux-gnu）体积较大（~210 MB），
原因是 jemalloc 静态链接且当前 release-dist 未启用 LTO。

**解决方案**：在 `Cargo.toml` 的 `[profile.release-dist]` 中添加 `lto = "thin"`
可显著减小二进制体积（预计降至 ~80 MB），但会增加编译时间。

## 供应商锁定

当前仅支持 Kimi Code 和 Moonshot 平台，暂未对接其他 LLM 供应商。
如需更多供应商支持，欢迎提交 PR 或发起 Discussion。

## 全量测试未全部通过

`cargo test --workspace --all-targets` 包含大量 E2E 测试和 PTY 相关测试，
部分测试对环境敏感（locale、终端类型等），在 CI 中目前只运行 `--lib` 级别的测试。

## 上游同步

本项目是 grok-build 的 hard fork，已对 crate 名称、二进制名、配置路径做了全量重命名。
上游 grok-build 发布新版本时，需要手动逐 commit 评估并 cherry-pick，同步成本较高。

---

## Linux binaries are large

Linux platform binaries (especially aarch64-unknown-linux-gnu) are large (~210 MB)
due to statically linked jemalloc and no LTO in the current release-dist profile.

**Workaround**: Add `lto = "thin"` to `[profile.release-dist]` in `Cargo.toml`
to significantly reduce binary size (~80 MB), at the cost of longer compile times.

## Single provider ecosystem

Currently only supports Kimi Code and Moonshot platforms. For additional provider
support, PRs and Discussions are welcome.

## Not all tests pass

`cargo test --workspace --all-targets` includes many E2E and PTY-dependent tests.
Some are environment-sensitive (locale, terminal type). CI currently only runs
`--lib` level tests.

## Upstream sync cost

This is a hard fork of grok-build with full crate renaming, binary renaming, and
independent config paths. Upstream grok-build releases require manual per-commit
evaluation and cherry-picking, which carries higher sync costs.
