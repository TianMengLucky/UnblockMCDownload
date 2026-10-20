此样例编译指定上游提交的 `RegionPolicy.cs`，并用一个小型更新器模拟原更新流程。
它用于验证内嵌程序集修改、本程序的命令行调用顺序和重复执行；不下载真实游戏或运行完整启动器。

复现时将提交 `8cd9d37491f08e1f8365622517aefed429b5e095` 的
`PCL.Core/App/Region/RegionPolicy.cs` 放到测试用 `Core` 项目中，程序集名设为 `PCL.Core`。
将本目录的 `Program.cs` 放入引用该 Core 项目的控制台项目，程序集名设为
`Plain Craft Launcher 2`，目标框架设为 `net9.0` 或更新版本。

```powershell
dotnet publish Host.csproj -c Release -r win-x64 --self-contained false -p:PublishSingleFile=true
# 再用 -p:EnableCompressionInSingleFile=true 验证压缩格式。

$env:UNBLOCKMCDOWNLOAD_CE_UPDATE_FIXTURE = 'C:\fixtures\Plain Craft Launcher 2.exe'
cargo test --locked --test unlock updates_are_patched -- --include-ignored
```

检查会通过本程序的 `--patch-update 下载文件 已安装启动器` 命令接入更新，再连续两次复用相同下载路径，验证新文件的地区策略、下一次更新入口，以及原始备份保留。
