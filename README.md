# Fenix to TFDI NavData Converter

将 Fenix `nd.db3` 导航数据库与中国民航 NAIP `RTE_SEG.csv` 转换为 TFDI MD-11 使用的 JSON 导航数据。

> 当前状态：测试版。转换结果已通过本地结构与引用完整性验证，但尚未完成 TFDI MD-11 实机验证，不得作为正式版发布。

## 数据边界

- `nd.db3` 和 `RTE_SEG.csv` 是导航内容输入。
- TFDI 官方 `Nav-Primary` 是目标 JSON 契约和全球数据模板，不是中国区域导航内容来源。
- 转换器只写入新的隔离候选目录，禁止将 `--output` 指向活动的游戏数据目录。
- 数据库、原始 CSV、候选输出、备份、日志和外部测试包均不提交到 Git。

## 构建

项目使用 Rust 2024 edition。Windows 上建议使用 GNU 工具链：

```powershell
rustup toolchain install stable-x86_64-pc-windows-gnu
cargo +stable-x86_64-pc-windows-gnu build --release
```

如果本机 MinGW 不在 `PATH` 中，需先将 `mingw64\bin` 加入当前终端的 `PATH`。中文路径环境下可以将 `CARGO_TARGET_DIR` 设置为纯英文临时目录。

## 转换

```powershell
.\target\release\fenix_to_tfdi.exe `
  --db "F:\我的世界动画\AI项目\导航数据\nd.db3" `
  --rte-seg "F:\我的世界动画\AI项目\导航数据\2607\RTE_SEG.csv" `
  --reference "C:\Users\Administrator\AppData\Roaming\Microsoft Flight Simulator 2024\WASM\MSFS2024\tfdidesign-aircraft-md11\work\Nav-Primary" `
  --output "F:\我的世界动画\AI项目\导航数据\fenix_to_tfdi\output\2607-test"
```

`--reference` 和 `--output` 必须是不同目录，且候选输出目录不得预先存在。程序会先复制完整官方模板，再替换目标区域数据并同步周期元数据。

## 自动验证

转换完成后会自动验证：

- 必需 JSON 存在且可解析。
- 主表 ID 唯一。
- Airport、Runway、Terminal、ILS、Navaid、Waypoint、Airway 和 Lookup 跨表引用完整。
- Terminal 与 `ProcedureLegs\TermID_<ID>.json` 一一对应。
- 程序航段的 `TerminalID`、`WptID`、`NavID` 和 `CenterID` 引用有效。
- `Config.json` 与 `cycle.json` 的周期一致。

任一验证失败都会返回非零退出状态，不会将候选输出标记为可部署版本。

## 测试

```powershell
cargo +stable-x86_64-pc-windows-gnu test --all-targets
```

## 来源与许可

本项目基于 [Yuzuriha03/Fenix2TFDINavDataConverter](https://github.com/Yuzuriha03/Fenix2TFDINavDataConverter) 的 GPL-3.0 代码继续开发，并参考同目录中的 Fenix to iniBuilds 与 Fenix to iFly 转换器进行数据校验和航路处理。

详细第三方说明见 `THIRD_PARTY_NOTICES.md`。本项目按照 GPL-3.0-only 许可发布。
