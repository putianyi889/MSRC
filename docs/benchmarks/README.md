# 算法 benchmark

每次增加算法版本，保留旧版本的压缩器、解压器和历史测量，追加新报告。
报告须同时注明算法编号、软件包版本、样本快照、构建方式、硬件和统计口径。

- [当前精简样本：1,038 份，RMV 2 保留 200 份](2026-10-27-v1-v2-reduced.md)
- [精简样本聚合 JSON](2026-10-27-v1-v2-reduced.json)
- [原始五轮测量：2,301 份样本](2026-10-27-v1-v2.md)
- [原始测量聚合 JSON](2026-10-27-v1-v2.json)

精简报告从同一次五轮实验的逐文件测量中筛选重汇总，并核对保留文件的内容哈希；
没有重新计时。原始报告与数据继续保留，样本选择不依据压缩率或耗时。

运行 [compare_algorithms.py](../../scripts/compare_algorithms.py) 可逐份验证还原，
比较当前保留的两个算法，并输出私有逐文件 CSV 和聚合 JSON：

```powershell
.\.venv\Scripts\python.exe scripts/compare_algorithms.py --rounds 5 --warmups 1 --level 6
```

默认输出为 `target/algorithm-comparison.csv` 和 `target/algorithm-comparison.json`。
原始录像和逐文件记录不提交；公开报告仅保留聚合统计。不同数据集、机器、构建或绑定的
数字不应直接混为一次测量；加入新样本后应生成新的快照或明确更新本次尚未发布的报告。
