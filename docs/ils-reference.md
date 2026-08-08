# 2607 ILS 参考补丁

Fenix 2608 NAIP 数据中发现部分中国机场保留了 ILS 台站，却缺少对应的 ILS 进近程序。`Fenix-2607有ILS版\nd.db3` 可作为旧周期程序参考，但不能替代 2608 的主数据源。

使用 `--ils-reference-db <PATH>` 时，转换器会在临时副本中执行受限补丁：

- 只处理 ICAO 以 `Z` 开头的中国区域机场；
- 仅在 2608 中找不到绑定该 ILS 台站的进近程序时补入；
- 要求机场、跑道、ILS 标识、频率和航向与 2608 完全匹配；
- 航点和导航台按标识与坐标重新映射到 2608，缺失的依赖记录才会复制；
- 不更新 2608 的机场、跑道、航路、SID、STAR、RNP 进近或 ILS 台站参数。

例如，ZUZH 的 `I20-Z ILS 20` 会在匹配跑道 20、`IPP`、频率和航向后导入。该补丁仍是测试版，必须完成 TFDI MD-11 实机验证后才能视为可用结果。

示例：

```powershell
fenix_to_tfdi.exe `
  --db "...\Navdata\nd.db3" `
  --ils-reference-db "...\Fenix-2607有ILS版\nd.db3" `
  --rte-seg "...\2608\RTE_SEG.csv" `
  --reference "...\Nav-Primary" `
  --output "...\candidate\Nav-Primary"
```
