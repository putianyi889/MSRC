# 本地真实录像样本

请把录像原文件放到这个目录，可以按格式建子目录，例如 `avf/`、`mvf/`、`rmv/`、
`evf/`、`rawvf/`。本目录仅提交 README 和忽略规则，录像、子目录及样本说明表都不会
被 Git 跟踪，也不会打进 Python/npm 发布包。

首批 **10–20 个文件**就足够；有多少提供多少，不必凑齐下面所有情况。

## 优先提供的样本

| 优先级 | 特征 | 目的 |
| --- | --- | --- |
| 高 | AVF、MVF、RMV、EVF，各 2–3 个；有 RAWVF 文本录像也可加入 | 比较不同原始格式的压缩率 |
| 高 | 正常完成的初级、中级、高级录像 | 建立常见录像的基线 |
| 高 | 很短或很小的录像，以及你手头最大的或鼠标事件最多的录像 | 测量固定容器开销和长录像的表现 |
| 中 | 同一格式的不同文件版本、不同录制软件版本 | 为后续按格式优化算法保留兼容性语料 |
| 中 | 失败、提前结束、含暂停或特殊操作的录像（如果软件支持） | 覆盖不同事件分布 |
| 中 | 自定义尺寸、非标准雷数，以及长时间、大量鼠标移动的录像 | 比较棋盘大小与事件数量的影响 |
| 可选 | 含中文或其他非 ASCII 名称、备注、扩展字段、签名或尾部附加数据的录像 | 验证完整字节恢复，并为未来可逆预处理留样本 |
| 可选 | 你已知难压缩、内部已有压缩或体积异常的录像 | 识别压缩后增大或收益较低的情况 |

文件本身比说明更重要。请直接提供原始录像；不要先转换成另一种录像格式或重新保存。
如有 ZIP/7z 打包，请把内部录像解出后放进来。暂时不需要专门制作损坏文件：截断、校验
错误和伪造长度已有合成测试。自然遇到的损坏文件可放在别处，之后单独研究。

V1 压缩原始字节，V2 对已识别事件布局做可逆预处理；这些真实样本用于测量压缩率和完整
字节恢复，不等同于验证录像合法性或可回放性。

格式预处理调研见[鼠标事件存储布局](../../docs/research/mouse-events/README.md)。
只读扫描当前已识别布局并生成 `target/mouse-event-summary.json`：

```powershell
.\.venv\Scripts\python.exe scripts/inspect_mouse_events.py
```

它支持 AVF、MVF 0.97/2006/2007、RMV 1/2、EVF 0.3/0.4、EVFS 0.0 的有限布局，
不执行回放或校验码验证；遇到未实现的版本、事件或不一致的长度会报错。
当前新增的 6 份 MVF 均为 0.97 beta，足够开展首轮实验。如继续补充样本，优先考虑
2006/2007、早期 MVF，以及含棋盘/指标事件的 EVF 0.4。
`rmv2/` 原始 1,463 份录像已确认文件头版本为 2，单独扫描和双版本往返验证均通过。
现保留 **200 份**，按棋盘/模式/NF 标志分组后取文件大小分位点，并补齐已观察到的
事件码、nFlags 值和半字节增量边界；最小/最大文件均保留。全样本集现有 **1,038 份**。
其余 1,263 份移至 `target/sample-archive/rmv2-2026-10-27/`，保持相对目录结构，
恢复时可移回 `tests/samples/rmv2/`。私有选样/移动清单为 `target/rmv2-reduction-plan.json`。

完整的版本对比见 [benchmark](../../docs/benchmarks/README.md)。安装当前 release 构建的
Python 绑定后，可运行以下只读比较；每份文件预热一次、计时五次，逐次检查恢复字节：

```powershell
.\.venv\Scripts\python.exe scripts/compare_algorithms.py --rounds 5 --warmups 1 --level 6
```

结果写入 `target/algorithm-comparison.csv` 和 `target/algorithm-comparison.json`，不覆盖
原有 `sample-report.csv`。它按文件头区分 RMV 1/2、EVF 0.3/0.4，并包含所有保留算法的压缩与解压。
当前[精简 benchmark](../../docs/benchmarks/2026-10-27-v1-v2-reduced.md) 直接重汇总原有五轮测量，
核对了保留文件的 SHA-256，没有重复计时；原五轮完整 CSV 保留为
`target/algorithm-comparison-full-2301.csv`。

## 可选说明

如果知道录制软件和版本，可在本目录创建 `samples.csv`；不知道的列留空即可。这个文件
同样会被忽略，不参与压缩测试。相对路径与文件对应即可，原文件名可以保留。

```csv
path,format_version,software,software_version,result,notes
avf/example.avf,,,,win,高级普通录像
rmv/long.rmv,,,,,手头事件最多的录像
```

## 本地检查与统计

本次初始化的 `.venv` 已安装 Python 绑定，在仓库根目录执行：

```powershell
.\.venv\Scripts\python.exe scripts/check_samples.py
```

其他环境需先按 [开发说明](../../docs/DEVELOPMENT.md) 安装绑定；激活环境后可使用
`python scripts/check_samples.py`。脚本递归读取本目录，依次压缩、解压，逐字节比较原文，
并生成被 Git 忽略的 `target/sample-report.csv`。不修改原文件，也不在样本目录写压缩包。

报告包含相对路径、原始 SHA-256、原始大小、压缩后大小（含容器头）、压缩率、节省比例、
压缩/解压耗时和检查状态。压缩率定义为 **压缩后大小 / 原始大小**，小于 1 表示变小；
空文件的比例留空。耗时是单次运行数据，适合初步比较，不是稳定的性能基准。

只测试默认 LZMA v1、级别 6；输出限制按原文件的实际大小设置，以容纳超过默认 64 MiB
限制的大样本。每次在内存里处理一个文件，不会并行读取全部录像。

也可以指定其他样本目录和报告位置：

```powershell
.\.venv\Scripts\python.exe scripts/check_samples.py D:\replays --report target/other-samples.csv
```
