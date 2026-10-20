# UnBlockMC

项目仓库：[TianMengLucky/UnBlockMC](https://github.com/TianMengLucky/UnBlockMC)

使用 Rust 2024 和 GPUI 的桌面软件项目，面向已编译的 Java 与 .NET 程序。
当前包含可启动的基础窗口和二进制编辑依赖，尚未实现文件打开、反编译或编辑界面。

## 依赖

| 库 | 用途 | 文档 |
| --- | --- | --- |
| `gpui` 0.2.2 | GPU 加速的原生桌面 UI | [GPUI](https://gpui.rs/) |
| `ristretto_classfile` 0.31 | 读取、修改、验证和写入 Java `.class` 字节码 | [API](https://docs.rs/ristretto_classfile/0.31.0/ristretto_classfile/) |
| `zip` 9.0.5 | 读取和重新打包 JAR，启用 Deflate 压缩 | [API](https://docs.rs/zip/9.0.5/zip/) |
| `dotscope` 0.9.1 | 读取、修改和写入 .NET 程序集元数据及 CIL | [项目](https://github.com/ATRAPSLLC/dotscope) |

`ristretto_classfile` 0.31 兼容本机 Rust 1.96；0.34 要求 Rust 1.97.1。
`dotscope` 仅启用核心功能，暂不启用模拟执行、反混淆、x86 分析等可选功能。
这里的 C# DLL/EXE 指含有 .NET 元数据的托管程序集；NativeAOT 或原生启动器不属于该编辑路径。

## 开发

Windows 环境需要 Rust MSVC 工具链、Visual Studio C++ Build Tools 和 Windows SDK。
依赖声明的最低 Rust 版本为 1.95，推荐使用已验证的 Rust 1.96 或更新版本。

```powershell
cargo run --locked
```

## 编译速度

默认开发构建与测试优先考虑编译速度：

- Windows 仅启用 GPUI 原生后端和应用清单，避免编译 Linux 的 Blade/Vulkan 后端；其他平台继续使用 GPUI 默认功能。
- 关闭开发构建的调试信息，使用 256 个代码生成单元并启用增量编译；测试继承相同设置。
- Windows x64 MSVC 使用 Rust 工具链内置的 `rust-lld.exe`，无需单独安装 LLVM。
- 发布构建仍使用 Cargo 默认优化配置。

关闭调试信息后，默认开发构建无法查看完整的调试器变量和源码回溯位置。
需要完整调试信息时，可在当前 PowerShell 会话临时恢复：

```powershell
$env:CARGO_PROFILE_DEV_DEBUG = "2"
cargo run --locked
Remove-Item Env:CARGO_PROFILE_DEV_DEBUG
```

日常检查优先使用 `cargo check --locked`，它不生成或链接应用可执行文件。
保留 `target` 缓存，不要在每次构建前运行 `cargo clean`。
切换上述编译设置会使已有缓存失效，首次重建耗时不能代表后续增量构建速度。

配置依据：[Cargo 构建性能指南](https://doc.rust-lang.org/cargo/guide/build-performance.html)、
[Cargo Profiles](https://doc.rust-lang.org/cargo/reference/profiles.html)。

本机实测（Windows x64、Rust 1.96、6 核 12 线程、16 GB 内存）：

| 项目 | 调整前 | 调整后 |
| --- | --- | --- |
| 入口文件触发的重编译 | 37.46 秒 | 7.60 秒 |
| Windows 依赖包数量 | 487 | 460 |

两次均预先编译依赖，仅更新 `src/main.rs` 的修改时间，再执行 `cargo build --locked --timings`。
这是单次开发重编译对比，耗时减少约 80%，不代表首次全量编译耗时。
原始记录保存在本地 `target/build-speed/before.json` 和 `after.json`，不纳入版本控制。

## 验证

验证格式、静态检查、依赖读写：

```powershell
cargo fmt --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
```

`tests/dependencies.rs` 验证 Java CLASS 修改后的 JAR 压缩读写，以及 .NET CIL 方法体生成。
它不代表对任意 JAR 或 .NET 程序集的兼容性保证。尚未进行运行时性能基准测试。

Git 已初始化，默认分支为 `main`；`Cargo.lock` 应纳入版本控制以固定依赖解析结果。

## 许可证

本项目采用 GNU Affero General Public License v3.0（`AGPL-3.0-only`）许可证，完整条款见 [LICENSE](LICENSE)。
第三方依赖仍遵循各自的许可证。

## 免责声明

- 使用者应自行确认对所处理的 Java / .NET 程序拥有合法权利或已取得相应授权，并遵守适用法律法规及第三方软件的许可条款。
- 修改二进制文件可能导致程序无法运行、数据丢失或数字签名失效，操作前请备份原始文件。
- 在适用法律允许的范围内，本项目按“现状”提供，不作任何明示或默示担保；作者及贡献者不对因使用、修改或分发本项目造成的损失承担责任。具体担保排除与责任限制以 [LICENSE](LICENSE) 为准。
