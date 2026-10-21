<div align="center">
  <img src="assets/icon.png" alt="UnblockMCDownload 项目标志" width="160" />
  <h1>UnblockMCDownload</h1>
  <p><strong>移除 PCL / HMCL / PCL-CE 的下载地区限制</strong></p>
  <p>选择启动器 EXE → 自动识别 → 备份并解锁</p>
  <p>
    <img src="https://img.shields.io/badge/version-0.0.3-286E5C?style=flat-square" alt="版本：0.0.3" />
    <img src="https://img.shields.io/badge/Windows-0078D4?style=flat-square" alt="平台：Windows" />
    <img src="https://img.shields.io/badge/Rust-2024-DEA584?style=flat-square" alt="Rust 2024" />
    <img src="https://img.shields.io/badge/GPUI-0.2.2-A78BFA?style=flat-square" alt="GPUI 0.2.2" />
    <a href="LICENSE"><img src="https://img.shields.io/badge/License-AGPL--3.0--only-A78BFA?style=flat-square" alt="许可证：AGPL-3.0-only" /></a>
  </p>
  <p>
    <a href="#快速开始">快速开始</a> ·
    <a href="#支持范围">支持范围</a> ·
    <a href="#自动更新">自动更新</a> ·
    <a href="#常见问题">常见问题</a> ·
    <a href="#开发与验证">开发与验证</a> ·
    <a href="#免责声明">免责声明</a>
  </p>
  <p>
    <a href="https://github.com/TianMengLucky/UnblockMCDownload/releases/latest">下载最新版</a> ·
    <a href="https://github.com/TianMengLucky/UnblockMCDownload">项目仓库</a> ·
    <a href="https://github.com/TianMengLucky/UnblockMCDownload/issues">反馈问题</a>
  </p>
</div>

**UnblockMCDownload** 是面向 Windows 的桌面工具，通过修改本机上受支持的启动器 EXE，移除下载相关的地区限制。当前版本为 **0.0.3**，使用 **Rust + GPUI** 构建，由 **Codex** 辅助开发。

| 功能 | 说明 |
| --- | --- |
| 自动识别 | 按文件内容识别 PCL、HMCL 或 PCL-CE |
| 备份与恢复 | 修改前保存为同目录下的 `.exe.bak`，已有备份不会被覆盖 |
| 重复处理检查 | 识别已解锁的文件，避免重复写入 |
| 后台处理 | 后台识别与修改，界面显示结果和备份路径 |
| 更新后继续解锁 | 可选修改启动器更新入口，在安装新版本前再次解锁 |
| 工具自身更新 | 启动时检查新版，下载并校验后自动替换、重启 |

## 快速开始

1. 从 [最新 Release](https://github.com/TianMengLucky/UnblockMCDownload/releases/latest) 下载 **`UnblockMCDownload.zip`** 并解压。
2. 双击 **`解锁MC下载限制.exe`**，阅读免责声明并点击「同意并继续」。
3. **完全退出目标启动器**，包括托盘进程。
4. 点击文件选择区域，选择启动器 **EXE**；如需在启动器更新后继续解锁，开启默认关闭的「修改自动更新」。
5. 点击 **「开始解锁」**，等待成功提示，再从原路径重新打开启动器。

原文件备份为同目录下的 `原文件.exe.bak`。首次同意记录保存在当前用户的配置目录，免责声明变更后会再次提示。从源码运行见 [开发与验证](#开发与验证)。

> [!IMPORTANT]
> 已有备份不会被覆盖。处理另一份原文件前，请先将旧备份另行保存。需要恢复时，关闭启动器，用 `.exe.bak` 备份替换修改后的 EXE。

<details>
<summary>校验下载文件（SHA-256）</summary>

ZIP 内包含 EXE 和对应的 `.sha256` 文件。在解压目录打开 PowerShell，运行：

```powershell
$expected = (Get-Content .\解锁MC下载限制.exe.sha256).Split(' ')[0]
(Get-FileHash .\解锁MC下载限制.exe -Algorithm SHA256).Hash -eq $expected
```

结果应为 `True`，表示 EXE 的 SHA-256 与发布的校验值一致。

</details>

## 界面与窗口操作

![UnblockMCDownload 应用截图](assets/screenshot.png)

界面采用半透明亚克力效果。拖动顶部标题区域移动窗口，右上角可最小化或退出；文件路径过长时可横向滚动。

| 快捷键 | 操作 |
| --- | --- |
| `Ctrl+O` | 选择启动器 EXE |
| `Ctrl+Enter` | 开始解锁 |
| `Ctrl+Shift+H` | 隐藏或恢复任务栏与 Alt+Tab 入口 |
| `Tab` / `Shift+Tab` | 切换按钮焦点 |

点击「隐藏任务栏」时窗口保持显示；点击「恢复任务栏」还原入口。最小化会自动恢复任务栏入口。

## 自动更新

### 工具自身更新

每次启动界面时，工具会在后台静默检查 GitHub 最新正式版。发现新版后自动下载并校验 SHA-256，等待当前解锁任务完成后退出、替换原 EXE 并重新启动。程序目录需要写入权限；检查或下载失败时继续使用当前版本。

更新请求优先经过 `https://gh-proxy.com/`，网络或校验失败后回退 GitHub 原始地址。启动器补下载缺失工具时也使用相同策略。

### 启动器更新后继续解锁

「修改自动更新」默认关闭。开启后，启动器会记录本工具的完整路径，在安装新文件前调用工具解锁，并接入下一次更新；退出工具界面后仍然有效。兼容性取决于目标方法与引用结构，具体见 [支持范围](#支持范围)。

| 情况 | 行为 |
| --- | --- |
| 工具仍在原路径 | 调用工具处理下载文件，成功后继续安装 |
| 工具已被删除或移动 | 从本项目最新 Release 下载 `MC.exe` 和 `MC.exe.sha256`，校验后保存到记录路径并调用 |
| 下载、校验、写入或解锁失败 | 停止本次启动器更新安装 |
| 希望更换工具路径或撤销接入 | 恢复启动器原始备份；如需重新接入，再使用新位置的工具处理 |

已解锁的文件也可再次开启此选项，程序会核对并保留匹配的原始备份。**关闭开关只影响本次操作，不会撤销已写入的更新入口。**

<details>
<summary>更新实现、备份与错误日志</summary>

- 启动器通过命令行调用当前工具，不创建额外的助手程序。PCL / PCL-CE 的缺失工具下载逻辑使用系统 Windows PowerShell，不另存脚本。
- Release 必须包含 `MC.exe` 和 `MC.exe.sha256`；HMCL 修改下载文件前仍执行原有签名校验。
- 更新中的临时下载文件在修改后原子替换，不另建 `.bak`，以便下次复用下载路径；首次解锁已安装启动器的原始备份会保留。
- 解锁失败记录在启动器旁的 `UnBlockMC.Update.log`；PCL / PCL-CE 补下载工具失败记录在工具原路径旁的 `.update.log`。
- 工具自身替换或重启失败时，错误记录在工具 EXE 旁的 `.update.log`。

</details>

## 支持范围

| 启动器 | 支持范围 | 已有验证 |
| --- | --- | --- |
| **PCL** | 按五个下载方法的签名、限制代码和引用关系匹配兼容版本；2.99.0.0 保留精确补丁作为快速路径 | 官方文件、旧版已开放逻辑、版本信息与成员引用顺序变化的兼容样例、备份和重复处理检查 |
| **HMCL** | 内含目标地区检测方法的 EXE，已验证 **3.17.0.362** | 官方 EXE 的补丁检查，其他归档条目逐项保持一致 |
| **PCL-CE** | 含目标地区判断逻辑的 .NET 单文件包，支持普通及 Deflate 压缩的内嵌程序集 | 使用指定提交源码构建样例，验证大陆、未知、其他地区三种结果及重复处理 |

> [!NOTE]
> 兼容性取决于文件内容，**不保证支持所有版本或未来版本**。无法匹配的版本、损坏文件、已签名 JAR，以及需要通用补丁处理但含本机代码的 PCL、核心含 ReadyToRun 本机代码的 PCL-CE 会报错并保留原文件。单文件大小上限为 **256 MiB**。尚未完成三款完整启动器的下载、安装全过程人工验证。

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

工具不按版本号限制处理范围。PCL 会按类型、方法签名和引用关系识别已知限制逻辑，再重新映射代码中的引用；HMCL 和 PCL-CE 按目标方法识别。若上游改写方法、移除依赖或使用不支持的本机代码，仍会拒绝修改。反馈时请附上启动器版本和完整错误提示。

</details>

## 开发与验证

### 构建环境

- Windows + Rust MSVC 工具链，项目声明最低 Rust 版本为 **1.95**。
- Visual Studio C++ Build Tools 与 Windows SDK。

若 Windows SDK 安装在非默认路径且 `fxc.exe` 不在 `PATH` 中，构建前将 `GPUI_FXC_PATH` 设置为实际的 `fxc.exe` 路径，供 GPUI 编译着色器使用。

```powershell
git clone https://github.com/TianMengLucky/UnblockMCDownload.git
cd UnblockMCDownload
cargo run --locked
# 构建发布版本：
cargo build --release --locked
```

默认启用 `gui` 功能。调试产物为 `target/debug/unblockmcdownload.exe`，发布产物为 `target/release/unblockmcdownload.exe`，发布时重命名为 `解锁MC下载限制.exe`。

### 自动构建与发布

[GitHub Actions](https://github.com/TianMengLucky/UnblockMCDownload/actions/workflows/build.yml) 会在分支推送、Pull Request 和手动触发时执行格式检查、Clippy、测试及 Windows x64 发布构建。构建成功后，可在对应运行的 Artifacts 中直接下载 `解锁MC下载限制.exe` 和 `解锁MC下载限制.exe.sha256`。

发布新版本时，同步修改 `Cargo.toml` 和 `Cargo.lock` 中本项目的版本号，再提交并推送到默认分支。工作流检测到版本变化后，会自动创建 `v版本号` 标签和 GitHub Release，无需手动创建标签或配置额外密钥。

<details>
<summary>发布附件与补发规则</summary>

- Release 包含 `MC.exe`、`MC.exe.sha256` 和 `UnblockMCDownload.zip`。ZIP 内保留中文文件名 `解锁MC下载限制.exe` 及其校验文件；自动更新使用直接附件。
- 上传前会验证 SHA-256；校验文件中的名称对应 ZIP 内的中文文件名。同一版本已有 Release 时更新附件。
- 默认分支首次推送会发布当前版本；其他分支、Pull Request 和默认的手动运行仅构建。
- 发布失败后，可在默认分支手动运行工作流并勾选 `publish_release`，补发当前版本，无需再次修改版本号。

</details>

### 常规检查

仅修改解锁逻辑时，可跳过 GUI、图像解码器和 Windows 资源编译器：

```powershell
cargo test --locked --no-default-features --lib --tests
cargo clippy --locked --no-default-features --lib --tests -- -D warnings
```

完整检查覆盖 GUI 和启动器更新逻辑：

```powershell
cargo fmt --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
```

发布配置使用 `opt-level = "s"` 并去除符号，不启用 LTO；开发配置保留增量编译，后续构建复用缓存。

默认最多并行编译 4 个任务，减少大依赖同时构建时的内存压力；可用 `cargo build -j 任务数` 或 `CARGO_BUILD_JOBS` 调整。
同一分支或 Pull Request 的新构建会取消尚未完成的旧构建，发布任务保留独立的并发控制。

发布时若更重视体积，可运行 `cargo build --release --locked --config profile.release.package.unblockmcdownload.codegen-units=1`；这会增加本项目的编译耗时。

大文件性能检查可单独运行，不需要 GUI 或真实启动器：

```powershell
cargo test --release --locked --no-default-features --lib benchmark_large_compressed_bundle -- --ignored --nocapture
```

该检查处理包含 16 MiB 压缩程序集的 64 MiB 单文件包，输出补丁耗时；默认测试跳过此检查。

[tests/unlock.rs](tests/unlock.rs) 检查备份、文件占用、错误输入、HMCL 字节码与 EXE 前缀保留。

自动更新检查还覆盖已有备份、重复接入、工具缺失时的下载与校验，以及关闭工具后连续两轮更新的样例验证。样例构建方式见 [tests/fixtures/ce](tests/fixtures/ce/README.md)，运行时将 `UNBLOCKMCDOWNLOAD_CE_UPDATE_FIXTURE` 指向构建出的 EXE。

工具自身更新的代理回退、SHA-256 校验、文件替换与重启检查见 [tests/self-update.ps1](tests/self-update.ps1)。

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

PCL 的精确补丁快速路径匹配以下官方 EXE 的 SHA-256：

```text
6b6e1260cb5a1d6a303855f1c60cab3d0bc1aebba7e40a964180188b271e7386
```

[tools/build_pcl_profile.py](tools/build_pcl_profile.py) 从官方 2.14.0.0 和 2.99.0.0 EXE 提取五个下载方法，重映射元数据令牌，生成 [pcl-2.99.0.0.bin](src/unlock/patches/pcl-2.99.0.0.bin)，保留 2.99 的其他代码、资源和版本信息。

通用匹配使用 [pcl-portable.json.gz](src/unlock/patches/pcl-portable.json.gz)。它记录方法签名、限制代码、可恢复代码及引用身份，不记录固定文件偏移。恢复时按目标文件重新查找引用、检查局部变量签名和异常处理表，并校验结果。只更新确认包含已知限制的下载方法；无法匹配时保留文件。

维护时可用同一对官方文件复现通用模板：

```powershell
$env:UNBLOCKMC_PROFILE_DONOR = 'C:\launchers\PCL-2.14.0.0.exe'
$env:UNBLOCKMC_PROFILE_RESTRICTED = 'C:\launchers\PCL-2.99.0.0.exe'
$env:UNBLOCKMC_PROFILE_OUTPUT = "$PWD\src\unlock\patches\pcl-portable.json.gz"
cargo test --locked --no-default-features --lib regenerate_portable_profile -- --ignored
```

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

GPT 娘形象参考来源：[Gender Swap AI](https://genderswapai.com/zh-CN/ai-gijinka/gpt)（原设定作者：ZipZipPipe）。
