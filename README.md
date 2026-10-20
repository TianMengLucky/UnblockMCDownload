<div align="center">
  <img src="assets/icon.png" alt="UnblockMCDownload 项目标志" width="160" />
  <p>GPT 娘形象参考来源：<a href="https://genderswapai.com/zh-CN/ai-gijinka/gpt">Gender Swap AI</a>（原设定作者：ZipZipPipe）</p>
  <h1>UnblockMCDownload</h1>
  <p><strong>移除 PCL / HMCL / PCL-CE 的下载地区限制</strong></p>
  <p>选择启动器，自动识别，备份后解锁。</p>
  <p>
    <img src="https://img.shields.io/badge/Windows-0078D4?style=flat-square" alt="平台：Windows" />
    <img src="https://img.shields.io/badge/Rust-2024-DEA584?style=flat-square" alt="Rust 2024" />
    <img src="https://img.shields.io/badge/GPUI-0.2.2-A78BFA?style=flat-square" alt="GPUI 0.2.2" />
    <a href="LICENSE"><img src="https://img.shields.io/badge/License-AGPL--3.0--only-A78BFA?style=flat-square" alt="许可证：AGPL-3.0-only" /></a>
  </p>
  <p>
    <a href="#这是什么">这是什么</a> ·
    <a href="#快速开始">快速开始</a> ·
    <a href="#支持范围">支持范围</a> ·
    <a href="#常见问题">常见问题</a> ·
    <a href="#开发与验证">开发与验证</a> ·
    <a href="#许可证">许可证</a>
  </p>
  <p>
    <a href="https://github.com/TianMengLucky/UnblockMCDownload">项目仓库</a> ·
    <a href="https://github.com/TianMengLucky/UnblockMCDownload/issues">反馈问题</a>
  </p>
</div>

## 这是什么

**UnblockMCDownload** 是一个面向 Windows 的桌面工具，用于修改本机上受支持的启动器 EXE，移除下载相关的地区限制。项目使用 **Codex** 辅助开发，以 **Rust** 编写，并基于 **GPUI** 构建界面。

| 功能 | 说明 |
| --- | --- |
| 自动识别 | 按文件内容识别 PCL、HMCL 或 PCL-CE，无需手动选择类型 |
| 自动备份 | 修改前将原文件保存为同目录下的 `.exe.bak`，已有备份不会被覆盖 |
| 避免重复修改 | 已解锁的文件再次处理时会直接提示，无需重新写入 |
| 后台处理 | 在后台完成识别与修改，界面显示处理结果和备份路径 |

## 界面与窗口操作

![UnblockMCDownload 应用截图](assets/screenshot.png)

- 操作面板采用半透明亚克力效果，带背景模糊、高光边缘和柔和阴影。
- 拖动顶部标题区域移动窗口；右上角提供自制最小化、退出按钮。
- Windows 下可点击“隐藏任务栏”移除本窗口的任务栏与 Alt+Tab 入口，窗口保持显示；再次点击“恢复任务栏”还原。最小化时会自动恢复任务栏入口。
- `Ctrl+Shift+H` 切换任务栏入口；`Tab` / `Shift+Tab` 切换按钮焦点。

## 快速开始

### 启动工具

从 [Releases](https://github.com/TianMengLucky/UnblockMCDownload/releases) 下载并双击 `解锁MC下载限制.exe`，或在仓库目录运行：

```powershell
cargo run --locked
```

首次从源码运行需准备 [构建环境](#开发与验证)。默认构建产物为 `target/debug/unblockmcdownload.exe`。

首次打开界面时会显示本 README 中的免责声明，点击「同意并继续」后才能使用；选择「不同意，退出」会关闭程序。同意记录保存在当前用户的配置目录，后续启动无需重复确认；免责声明变更后会再次提示。

### 校验下载文件

从同一 Release 下载 `解锁MC下载限制.exe` 和 `解锁MC下载限制.exe.sha256`，在下载目录打开 PowerShell，运行：

```powershell
$expected = (Get-Content .\解锁MC下载限制.exe.sha256).Split(' ')[0]
(Get-FileHash .\解锁MC下载限制.exe -Algorithm SHA256).Hash -eq $expected
```

结果应为 `True`，表示 EXE 的 SHA-256 与发布的校验值一致。

### 解锁启动器

1. **完全退出目标启动器**，包括可能仍在运行的托盘进程。
2. 点击 **「选择文件…」**，选择启动器的 EXE。界面会显示完整路径，长路径可横向滚动查看。
3. 如需更新后继续解锁，打开按钮下方的 **「修改自动更新」**；此选项默认关闭。
4. 点击 **「开始解锁」**，等待界面提示「下载限制已移除」。
5. 从原路径重新打开启动器。原文件已保存为同目录下的 `原文件.exe.bak`。

快捷键：<kbd>Ctrl</kbd> + <kbd>O</kbd> 选择文件，<kbd>Ctrl</kbd> + <kbd>Enter</kbd> 开始解锁。

> [!IMPORTANT]
> 已有备份不会被覆盖。处理另一份原文件前，请先将旧备份另行保存；需要撤销修改时，请关闭启动器，再用备份替换修改后的 EXE。

### 修改自动更新

打开此选项后，程序会修改启动器的更新安装入口。启动器下载新文件后，通过命令行调用当前的 `解锁MC下载限制.exe`，先解锁新文件并接入下一次更新，再继续安装。不会创建额外的助手程序；工具仅在更新时启动，退出界面后仍然有效。

启动器会记录本工具的完整路径。如果工具文件不存在，会从本项目 GitHub 最新 Release 下载 `解锁MC下载限制.exe` 及其 `.sha256` 校验文件，校验通过后保存到原记录路径并调用。Release 必须包含这两个附件；网络、写入权限、校验或修改失败时，都会停止本次更新安装。下载回退使用启动器内嵌的代码，PCL / PCL-CE 使用系统 Windows PowerShell，不另存脚本文件。

HMCL 在修改下载文件前仍执行原有签名校验。工具处理失败的错误记录在启动器旁的 `UnBlockMC.Update.log`；PCL / PCL-CE 回退下载失败记录在工具原路径旁的 `.update.log`。此功能沿用下方支持范围，**不保证兼容尚未适配的未来版本**，尤其是严格匹配文件哈希的 PCL。

临时下载文件会在完成修改后原子替换，不创建 `.bak`，以便启动器下次仍可使用相同下载路径；首次解锁已安装启动器时创建的原始备份会保留。

如需修改记录的工具路径，请先恢复启动器的原始备份，再使用新位置的工具重新处理。

已使用本工具解锁的文件也可以再次开启此选项；程序会检查现有原始备份与当前文件是否匹配，并保留原备份。关闭界面上的开关只影响本次操作；撤销已写入启动器的更新接入点，需要恢复原备份。

## 支持范围

| 启动器 | 支持范围 | 已有验证 |
| --- | --- | --- |
| **PCL** | 官方 **2.99.0.0**，严格匹配完整 SHA-256 | 官方 EXE 的备份、五个方法体和重复处理检查 |
| **HMCL** | 内含目标地区检测方法的 EXE，已验证 **3.17.0.362** | 官方 EXE 的补丁检查，其他归档条目逐项保持一致 |
| **PCL-CE** | 含目标地区判断逻辑的 .NET 单文件包，支持普通及 Deflate 压缩的内嵌程序集 | 使用指定提交源码构建样例，验证大陆、未知、其他地区三种结果及重复处理 |

> [!NOTE]
> 无法匹配的版本、损坏文件、已签名 JAR 和核心含 ReadyToRun 本机代码的 PCL-CE 会报错并保留原文件。单文件大小上限为 **256 MiB**。尚未完成三款完整启动器的下载、安装全过程人工验证。

<details>
<summary>查看补丁范围与对应上游提交</summary>

- **PCL**：恢复五处被编译删除的下载 / 整合包逻辑。对应 [PCL 6b9a50d](https://github.com/Meloong-Git/PCL/commit/6b9a50df911c9d68b69b24c979efba477fb14b67)。
- **HMCL**：修改 `RegionUtils.checkChinaMainlandNetwork()Z`，仅涉及下载相关的网络地区判断。对应 [HMCL 8827fd0](https://github.com/HMCL-dev/HMCL/commit/8827fd06d8ea8bcd899e54f76004f63a447e2601)。
- **PCL-CE**：修改 `PCL.Core.App.Region.RegionAssessment.IsRestricted`。对应 [PCL-CE 8cd9d37](https://github.com/PCL-Community/PCL-CE/commit/8cd9d37491f08e1f8365622517aefed429b5e095)。

</details>

## 常见问题

<details>
<summary>提示无法打开目标文件，怎么办？</summary>

请完全退出启动器及其托盘进程，并确认对目标 EXE 和所在目录拥有写入权限，再重新处理。

</details>

<details>
<summary>提示备份已存在，怎么办？</summary>

先将同目录下的 `原文件.exe.bak` 另行保存，再处理新的原文件。程序会保留现有备份，不会直接覆盖它。

</details>

<details>
<summary>如何恢复原始启动器？</summary>

关闭启动器，将同目录下的 `原文件.exe.bak` 改回原文件名，替换修改后的 EXE 即可。

</details>

<details>
<summary>为什么启动器版本相近，仍然无法识别？</summary>

补丁需要匹配文件内容，并非所有版本都支持。PCL 目前严格匹配官方 2.99.0.0 的文件哈希；HMCL 和 PCL-CE 也需要包含对应的目标方法。请先核对 [支持范围](#支持范围)，反馈时附上启动器版本和完整错误提示。

</details>

## 开发与验证

### 构建环境

- Windows + Rust MSVC 工具链，项目声明最低 Rust 版本为 **1.95**。
- Visual Studio C++ Build Tools 与 Windows SDK。

```powershell
git clone https://github.com/TianMengLucky/UnblockMCDownload.git
cd UnblockMCDownload
cargo build --release --locked
```

Cargo 原始构建产物为 `target/release/unblockmcdownload.exe`，发布时重命名为 `解锁MC下载限制.exe`。

### 自动构建与发布

[GitHub Actions](https://github.com/TianMengLucky/UnblockMCDownload/actions/workflows/build.yml) 会在分支推送、Pull Request 和手动触发时执行格式检查、Clippy、测试及 Windows x64 发布构建。构建成功后，可在对应运行的 Artifacts 中直接下载 `解锁MC下载限制.exe` 和 `解锁MC下载限制.exe.sha256`。

默认分支的推送中，若 `Cargo.toml` 的 `[package].version` 相比推送前发生变化，工作流会自动创建 `v版本号` 标签和 GitHub Release，并附上 EXE 和 SHA-256 校验文件；发布前会再次验证校验值，同一版本已有 Release 时跳过发布。首次推送也会发布当前版本，其他分支、Pull Request 和手动运行仅构建。

发布新版本时，同时更新 `Cargo.toml` 和 `Cargo.lock` 中本项目的版本号（可在修改 `Cargo.toml` 后运行 `cargo check` 自动同步锁文件），再提交并推送到默认分支即可，无需手动创建标签或配置额外密钥。

### 常规检查

仅修改解锁逻辑时，可跳过 GUI、图像解码器和 Windows 资源编译器：

```powershell
cargo test --locked --no-default-features --lib --tests
cargo clippy --locked --no-default-features --lib --tests -- -D warnings
```

默认启用 `gui`，正常构建仍生成桌面应用；完整检查会覆盖 GUI 和更新子进程：

```powershell
cargo fmt --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
```

发布配置使用 `opt-level = "s"` 并去除符号，优先缩小 EXE；不启用 LTO，以控制构建耗时。
开发配置保留增量编译。首次更改构建配置会重新编译依赖，后续构建复用缓存。

[tests/unlock.rs](tests/unlock.rs) 检查备份、文件占用、错误输入、HMCL 字节码与 EXE 前缀保留。

自动更新检查还覆盖已有备份、重复接入、工具缺失时的下载与校验，以及关闭工具后连续两轮更新的样例验证。样例构建方式见 [tests/fixtures/ce](tests/fixtures/ce/README.md)，运行时将 `UNBLOCKMCDOWNLOAD_CE_UPDATE_FIXTURE` 指向构建出的 EXE。

HMCL 内嵌更新桥的 Java 源码位于 [UnBlockMcUpdate.java](src/unlock/patches/UnBlockMcUpdate.java)。维护时可用 `javac --release 8 -d 临时目录 src/unlock/patches/UnBlockMcUpdate.java` 编译，将包目录中的 `.class` 更新到同目录；日常构建和运行不需要 Java 编译器。

<details>
<summary>使用本地启动器运行集成检查</summary>

真实启动器检查默认忽略，需要提供本地文件。测试会自行创建临时副本：

```powershell
$env:UNBLOCKMCDOWNLOAD_PCL_EXE = 'C:\launchers\PCL-2.99.0.0.exe'
$env:UNBLOCKMCDOWNLOAD_HMCL_EXE = 'C:\launchers\HMCL-3.17.0.362.exe'
$env:UNBLOCKMCDOWNLOAD_PCL_CE_EXE = 'C:\launchers\PCL-CE.exe'
cargo test --locked --test unlock -- --include-ignored --test-threads=1
```

</details>

<details>
<summary>复现 PCL 内置二进制补丁</summary>

PCL 补丁严格匹配原始 EXE 的 SHA-256：

```text
6b6e1260cb5a1d6a303855f1c60cab3d0bc1aebba7e40a964180188b271e7386
```

[tools/build_pcl_profile.py](tools/build_pcl_profile.py) 从官方 2.14.0.0 和 2.99.0.0 EXE 提取五个下载方法，重映射元数据令牌，生成 [pcl-2.99.0.0.bin](src/unlock/patches/pcl-2.99.0.0.bin)，保留 2.99 的其他代码、资源和版本信息。

复现方式见脚本开头。Python 和相关包仅用于维护补丁，运行 UnblockMCDownload 不需要它们。

</details>

## 免责声明

使用本项目前，请阅读并理解以下条款：

1. **使用目的**：本项目仅供个人研究与学习使用。使用者应自行确保其使用行为符合适用法律法规及相关应用的许可协议和服务条款。
2. **责任承担**：使用者应自行评估使用本项目的风险，并对使用行为及其造成的一切后果负责。在适用法律允许的最大范围内，项目作者及其贡献者不对因使用、修改或分发本项目而产生的任何直接或间接损失、纠纷及其他后果承担责任。
3. **无担保**：本项目按“现状”提供，不作任何明示或默示担保，包括功能可用性、兼容性、稳定性及特定用途适用性。具体担保排除与责任限制以 [LICENSE](LICENSE) 为准。
4. **独立项目**：本项目与 HMCL、PCL-CE、PCL 及其开发者、维护者或所属组织无关联，未获得其授权、认可或赞助。文中提及的应用名称仅用于说明本项目的用途，相关名称及商标归各自权利人所有。

## 许可证

本项目采用 GNU Affero General Public License v3.0（`AGPL-3.0-only`）许可证，完整条款见 [LICENSE](LICENSE)。第三方依赖遵循各自的许可证。
