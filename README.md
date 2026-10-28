# MSRC

用 Rust 实现的扫雷录像无损压缩库，提供 Python 和 JavaScript/TypeScript（WebAssembly）
绑定。当前版本为 `0.2.0`，默认使用 **字段分流＋定宽差分/ZigZag＋LZMA1（V2）**。
已发布的 `0.1.0` 使用原始字节 LZMA1（V1）；V1 的压缩和解压继续保留。
Python 安装包通过 [GitHub Releases](https://github.com/putianyi889/MSRC/releases) 分发；
JavaScript/TypeScript 包以 `@putianyi888/msrc` 名称公开发布到 npm。

压缩对象是录像文件的**完整原始字节**：AVF、MVF、RMV、EVF、RAWVF 等格式都可原样
压缩。V2 对已识别的事件布局做可逆预处理，其他情况按原始字节压缩；不丢弃扩展字段，
也不判断输入是否为合法录像。文件名、路径和
扩展名由调用方保存。每个 `.msrc` 容器只保存一个文件的内容，小文件或已有压缩的数据
可能压缩后更大。

## Rust

在本仓库中运行 `cargo test --locked`，或从其他项目通过 path 依赖引入根 crate。

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
let replay = std::fs::read("game.avf")?;
let archive = msrc::compress(&replay)?;
std::fs::write("game.avf.msrc", &archive)?;
assert_eq!(msrc::decompress(&archive)?, replay);

let info = msrc::inspect(&archive)?;
assert_eq!(info.algorithm()?, msrc::Algorithm::LzmaV2);
Ok(())
}
```

`compress_with_options` 可设置算法及 `0..=9` 压缩级别，默认级别为 6。
选择旧版压缩器时使用 `CompressionOptions { algorithm: Algorithm::LzmaV1, ..Default::default() }`。
`decompress_with_limits` 可分别设置输出大小及解码器工作内存限制。

## Python

需要 Python 3.10+。从 [GitHub Releases](https://github.com/putianyi889/MSRC/releases)
下载与你的操作系统和架构匹配的 `.whl` 文件，再用 pip 安装。安装 wheel 不需要 Rust。
例如 Windows x64：

```sh
python -m pip install ./msrc-0.2.0-cp310-abi3-win_amd64.whl
```

也可以直接使用 `0.2.0` 的 Release 资产 URL：

```sh
python -m pip install "https://github.com/putianyi889/MSRC/releases/download/v0.2.0/msrc-0.2.0-cp310-abi3-win_amd64.whl"
```

Python 包的分发渠道是本仓库的 GitHub Release，安装时请指定 wheel 文件或资产 URL。
Release 同时提供源码包和 `SHA256SUMS`。从源码构建时需要 Rust 工具链及本机链接器：

```sh
python -m venv .venv
# 激活 .venv（Windows PowerShell: .venv\Scripts\Activate.ps1）
python -m pip install "maturin>=1.9,<2"
maturin develop --release --locked
python -m unittest discover -s tests -p "test_python.py"
```

```python
from pathlib import Path
import msrc

replay = Path("game.rmv").read_bytes()
archive = msrc.compress(replay, level=6)
assert msrc.decompress(archive) == replay
print(msrc.inspect(archive).algorithm_version)  # 2
legacy_archive = msrc.compress(replay, algorithm_version=1)
assert msrc.decompress(legacy_archive) == replay

# 默认输出限制 64 MiB；只有需要时才调大。
restored = msrc.decompress(archive, max_output_size=128 * 1024 * 1024)
```

输入、输出均为 `bytes`，压缩与解压期间释放 GIL。无效容器、未知算法或超出解压限制
会抛出 `ValueError`；参数类型或整数范围不正确会抛出 Python 参数转换错误。

## JavaScript / TypeScript

需要 Rust、`wasm32-unknown-unknown` 目标、wasm-pack 0.15 和 Node.js 22+。

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-pack --version 0.15.0 --locked
npm ci
npm run build
npm test
npm run pack:check
```

构建结果位于 `dist/npm`，包含 Node.js、浏览器 ESM、WASM 和自动生成的 TypeScript
类型声明；可通过 `npm install ./dist/npm` 安装到其他项目。

可通过 `npm install @putianyi888/msrc` 安装。npm scope 是 `putianyi888`，
GitHub 仓库所有者为 `putianyi889`。需要本次版本时可显式安装
`npm install @putianyi888/msrc@0.2.0`。

Node.js：

```js
const { compress, decompress, inspect } = require("@putianyi888/msrc");
const replay = new Uint8Array([0, 1, 2, 255]);
const archive = compress(replay); // 默认 level=6、algorithm_version=2
const legacyArchive = compress(replay, 6, 1); // 显式使用 V1
decompress(legacyArchive);
const restored = decompress(archive); // Uint8Array
const info = inspect(archive);
try {
  console.log(info.original_size); // bigint
} finally {
  info.free();
}
```

浏览器使用显式 ESM 入口（构建工具需要能部署相邻的 `.wasm` 文件）：

```js
import init, { compress, decompress } from "@putianyi888/msrc/web";
await init();
const archive = compress(new Uint8Array(await file.arrayBuffer()));
const restored = decompress(archive);
```

可用 `init({ module_or_path: wasmBytes })` 或显式 WASM URL 初始化。
`decompress(archive, maxOutputSize?, maxMemoryKiB?)` 的后两个参数默认为 67108864 和
65536，JS 中上限均为 `2**32 - 1`，要求非负整数。API 为同步计算，大录像可放进 Worker。

## 容器与兼容性

容器版本、算法 ID、算法版本和软件包版本分别管理。首版为容器版本 `1`、算法 ID `1`
（LZMA1）、算法版本 `1`。`0.2.0` 增加同一算法 ID 的版本 `2`，容器头保持不变。
解压依据文件头自动选择对应算法；未知版本明确报错。
后续增加新算法时保留所有已有版本的压缩器和解压器。

容器包含原始大小、压缩大小及三个 CRC-32 校验值。默认解压限制为 **64 MiB 输出**和
**64 MiB 解码器工作内存**；工作内存是底层 LZMA 的估算，不是进程总内存上限，绑定
还会复制输入/输出。可压缩超过默认解压限制的文件，恢复时需显式提高限制。
V2 还需临时保存一个事件段及其类型列，工作内存上限不包含这部分缓冲区。
所有 API 在完整校验通过后才返回解压数据。`inspect` 仅校验容器及压缩载荷，不校验
解压结果，且允许查看未知算法的元数据。CRC 用于检测损坏，不提供身份验证或加密。

- [容器二进制规范与算法版本规则](docs/FORMAT.md)
- [现有格式与依赖调研](docs/RESEARCH.md)
- [鼠标事件存储布局与压缩方向](docs/research/mouse-events/README.md)
- [V2 定宽差分与 ZigZag 载荷规范](docs/algorithms/lzma-v2.md)
- [各算法版本的压缩率与性能 benchmark](docs/benchmarks/README.md)
- [构建、测试与发布配置](docs/DEVELOPMENT.md)

## 验证与发布

CI 构建 Linux、Windows、macOS Python wheel、源码包及 npm tarball，运行 Rust 检查、
Python 测试和 Node.js/WASM 测试，并上传产物。普通 push、PR 和 tag 均不会发布。
在版本 tag 上手动执行工作流：`publish_python` 将 wheel、源码包和 `SHA256SUMS` 上传
到 GitHub Release，使用工作流自带的 GitHub Token；`publish_npm` 将 JS 包发布到 npm，
使用 npm Trusted Publishing（OIDC），无需配置 npm Token。两个选项默认关闭，详见开发文档。

CI 测试使用独立 Python 标准库生成的兼容性样本和合成字节；真实录像的本地检查参见
[样本说明](tests/samples/README.md)，个人样本不随仓库或安装包分发。浏览器 ESM 在
Node.js 中执行验证，实际浏览器验证应单独进行；本仓库没有 Cypress。

MIT License.

