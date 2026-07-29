# Fenix -> TFDI 导航数据转换工具

```text
===============================================================================
  Fenix -> TFDI 导航数据转换工具
  中国区域数据补充 (NAIP)
  状态: 开发测试版 / 未经 TFDI MD-11 实机验证
  https://github.com/JCH2333/fenix_to_tfdi
===============================================================================
```

> **重要提示：本项目当前只是测试版。**
>
> 2607 候选数据已通过本地 JSON 解析、ID 唯一性、跨表引用、程序文件和周期元数据验证，但尚未在 TFDI MD-11 中完成机场输入、SID/STAR/IAP 选择、退出飞行和退出游戏测试。
>
> 只能作为外部测试候选使用，不得当作正式版。


## 一、简介

本工具将 Fenix A320 的 `nd.db3` 导航数据与中国民航 NAIP
`RTE_SEG.csv` 转换为 TFDI MD-11 使用的 `Nav-Primary` JSON 导航数据。

转换原则：

- `nd.db3` 和 `RTE_SEG.csv` 提供导航内容。
- TFDI 官方 `Nav-Primary` 提供 JSON schema、字段顺序、文件布局和全球模板数据。
- 中国区域记录按明确键替换，保留模板中其他全球数据。
- 程序默认生成新的隔离候选目录，不会自动覆盖游戏 WASM 文件。

当前 2607 候选数据的本地验证结果：

- Airports: 17,347
- Runways: 42,906
- Terminals / ProcedureLegs 文件: 101,690
- Navaids: 11,536
- Waypoints: 329,891
- Airways: 10,338
- AirwayLegs: 158,061
- ILSes: 4,608
- ProcedureLegs 航段: 846,154
- Cycle: 2607，Revision: 2
- 本地完整性验证: 通过
- TFDI MD-11 实机验证: **未进行**


## 二、系统要求

- Windows 10/11
- Python 3.10 或更高版本（运行 GUI）
- Rust stable（仅从源码构建时需要）
- 已安装 TFDI MD-11
- 含 NAIP 中国数据的 Fenix `nd.db3`
- 对应周期的 `2607/RTE_SEG.csv`
- 一份已知可正常使用的 TFDI 官方 `Nav-Primary` 模板


## 三、GUI 快速开始（推荐）

1. 先按下一节构建 `fenix_to_tfdi.exe`，或者将已经构建好的
   `fenix_to_tfdi.exe` 放在 `gui.py` 同一目录。
2. 双击 `run_gui.bat` 启动图形界面。
3. 点击“自动检测路径”，检查以下四项：
   - Fenix `nd.db3`
   - `RTE_SEG.csv`
   - TFDI 官方 `Nav-Primary` 模板
   - 新的候选输出目录
4. 点击“开始转换”，阅读测试版提示并确认。
5. 等待转换器完成生成和内置验证；成功后点击“打开输出目录”。

GUI 会优先查找同目录的 `fenix_to_tfdi.exe`，其次查找
`target/release/fenix_to_tfdi.exe`。路径自动检测会检查项目附近的输入数据，
并检查 MSFS 2024 WASM 中通常使用的 TFDI 目录。

GUI 只调用现有 Rust 转换核心，不会自行改写数据规则。它只允许输出到尚不存在的
新目录，不提供直接覆盖 WASM 的按钮。界面显示“转换及验证完成”只表示通过本地
验证，**不表示已经通过 TFDI MD-11 实机验证**。


## 四、构建

Windows 上建议使用 GNU 工具链：

```powershell
rustup toolchain install stable-x86_64-pc-windows-gnu
cargo +stable-x86_64-pc-windows-gnu build --release
```

如果 MinGW 不在 `PATH` 中，需先将 `mingw64\bin` 加入当前终端。
项目所在路径含中文时，建议将 `CARGO_TARGET_DIR` 设置为纯英文临时目录。


## 五、命令行转换（高级）

```powershell
.\target\release\fenix_to_tfdi.exe `
  --db "F:\我的世界动画\AI项目\导航数据\nd.db3" `
  --rte-seg "F:\我的世界动画\AI项目\导航数据\2607\RTE_SEG.csv" `
  --reference "C:\Users\Administrator\AppData\Roaming\Microsoft Flight Simulator 2024\WASM\MSFS2024\tfdidesign-aircraft-md11\work\Nav-Primary" `
  --output "F:\我的世界动画\AI项目\导航数据\fenix_to_tfdi\output\2607-test"
```

参数：

```text
--db PATH          Fenix nd.db3 输入
--rte-seg PATH     NAIP RTE_SEG.csv 输入
--reference DIR    官方 TFDI Nav-Primary 模板
--output DIR       新的隔离候选输出目录
--help             显示帮助
```

`--reference` 与 `--output` 必须不同，且 `--output` 不得预先存在。
程序不会自动探测或覆盖活动的游戏目录。


## 六、自动验证

转换完成后会自动执行 TFDI 专用验证，任一检查失败都会返回非零退出状态：

- 必需 JSON 存在且可解析。
- 主表 ID 唯一。
- Airport、Runway、Terminal、ILS、Navaid、Waypoint、Airway 和 Lookup 引用完整。
- Terminal 与 `ProcedureLegs\TermID_<ID>.json` 一一对应。
- 程序航段的 `TerminalID`、`WptID`、`NavID` 和 `CenterID` 引用有效。
- `Config.json` 与 `cycle.json` 的周期一致。

本地验证只能证明输出符合已知的 TFDI 文件契约，不能代替实机验证。


## 七、测试安装到 MSFS 2024

TFDI 活动数据通常位于：

```text
%APPDATA%\Microsoft Flight Simulator 2024\WASM\MSFS2024\
  tfdidesign-aircraft-md11\work\Nav-Primary
```

**只有在完全退出 MSFS 2024 后才能进行以下操作。**

1. 在任务管理器中确认 `FlightSimulator2024.exe` 不存在。
2. 将整个活动 `Nav-Primary` 目录复制到游戏目录之外的时间戳备份目录。
3. 确认备份中同时包含 `Config.json`、`cycle.json`、所有主 JSON 和 `ProcedureLegs` 目录。
4. 删除或移出旧的活动 `Nav-Primary`，不要把新文件直接叠加到旧目录。
5. 将已通过验证的候选目录整体复制为新的 `Nav-Primary`。
6. 确认新目录包含 17 个根文件、101,690 个 ProcedureLegs 文件，且周期为 2607 / 修订 2。
7. 启动 MSFS 2024 进行实机测试。

当前工具故意不提供“直接覆盖”参数，避免在模拟器运行时破坏 WASM 数据。


## 八、恢复官方数据

1. 完全退出 MSFS 2024。
2. 将当前测试用 `Nav-Primary` 移出 WASM 目录。
3. 将时间戳备份整体复制回原位，目录名恢复为 `Nav-Primary`。
4. 确认 `Config.json`、`cycle.json` 和 `ProcedureLegs` 已恢复。
5. 再启动 MSFS 2024。


## 九、建议的实机测试

1. 在 FMS 中输入 `ZBCF`、`ZUNZ`、`ZUUU`，确认机场可检索且不重复。
2. 分别设置为出发和到达机场。
3. 打开程序页，选择 SID、STAR 和 IAP。
4. 检查跑道过渡、公共段、RF 航段和复飞段。
5. 退出飞行后再进入一次。
6. 最后退出游戏，确认没有延迟崩溃。


## 十、文件说明

```text
src/source/fenix.rs       Fenix 数据源元数据解析
src/model.rs              与目标机模无关的中间模型
src/adapter/tfdi.rs       TFDI 周期适配、安全写入和专用验证
src/terminal_legs.rs      TFDI ProcedureLegs 生成与引用重映射
src/airways/              RTE_SEG 解析与航路合并
tests/                    契约、CLI 和回归测试
gui.py                    面向用户的 Tkinter 图形界面
gui_logic.py              GUI 路径检测、校验与命令构造
python_tests/             GUI 公共逻辑测试
run_gui.bat               Windows 双击启动脚本
docs/tfdi-contract.md     已检查的 TFDI 运行时契约
```


## 十一、注意事项

1. 当前候选只能标记为“测试版 / 未经实机验证”。
2. 不要在 MSFS 2024 运行时覆盖 WASM 文件。
3. 覆盖前必须备份整个 `Nav-Primary`，不能只备份 `cycle.json`。
4. 候选目录必须整体替换，不能与旧 ProcedureLegs 混合。
5. 每个 AIRAC 周期必须使用对应的 Fenix 数据和 TFDI 官方模板重新生成。
6. 没有实机结果前不会创建正式版本号、Git tag 或 GitHub Release。
7. 输入数据库、官方模板、生成候选、备份、日志和外部测试包均不提交到 GitHub。


## 十二、参考项目

- [Yuzuriha03/Fenix2TFDINavDataConverter](https://github.com/Yuzuriha03/Fenix2TFDINavDataConverter)
- [JCH2333/fenix_to_ini](https://github.com/JCH2333/fenix_to_ini)

详细第三方代码来源见 `THIRD_PARTY_NOTICES.md`。

```text
===============================================================================
  License: GPL-3.0-only
  Author:  JCH2333
  GitHub:  https://github.com/JCH2333/fenix_to_tfdi
===============================================================================
```
