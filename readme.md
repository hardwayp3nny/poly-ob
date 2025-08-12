# poly-ob：分布式 Polymarket 订单簿抓取框架

一个高性能、分布式的订单簿抓取系统。整体分为一个 Client Node（调度/健康检查/下发批量指令）与 N 个 Fetch Node（批量抓取/写入 Redis）。Fetch Node 通过批量请求 Polymarket `/books` 接口，按 token 粒度进行原子“最新快照”更新，避免积压历史、控制数据库尺寸。

## 主要特性
- 每个 Fetch Node 峰值 ≤ 20 请求/秒；集群容量 = 20 × N 请求/秒
- 批量抓取：一次 `/books` 请求可携带多个 tokenId，显著降低 HTTP 次数
- 原子写入：Redis Lua CAS，按 token 仅维护“最新快照”（覆盖旧值）
- 精确调度：全局时间片 Δ = 1/(20×N) 秒，均匀分配到各 Fetch Node
- 低延时通信：Client → Fetch 走 TCP（长度前缀 JSON），Fetch 默认监听 3000 端口

## 目录结构
poly-ob/
├─ crates/
│ ├─ common/ # 公共库：HTTP、Redis、类型、配置、Lua 脚本
│ ├─ client/ # Client Node：调度、健康检查、下发批量指令
│ └─ fetcher/ # Fetch Node：接收指令、批量请求 /books、写入 Redis
├─ client_config.example.toml
├─ fetch_config.example.toml
└─ README.md


## 依赖
- Rust 1.75+（Edition 2021）
- Redis（建议同机房/同内网）
- 所有主机开启 NTP，保证时间同步

## 构建
```bash
cd poly-ob
cargo build --release
```

## 配置
- 复制示例并按需修改：
```bash
cp client_config.example.toml client_config.toml
cp fetch_config.example.toml  fetch_config.toml
```

- `client_config.toml`
```toml
redis_url = "redis://127.0.0.1:6379"
base_url  = "https://clob.polymarket.com"

# 需要追踪的 token 全量列表（仅保存最新快照）
tokens = [
  "30392948954970070917163171791403085239525537531204487105908435228957543147284",
  # ...
]

# Fetch 节点的 TCP 地址（ip:port，默认 3000）
fetch_nodes = [
  "10.0.0.1:3000",
  # ...
]

# 调度滚动窗口（秒）
plan_horizon_secs = 5
```

- `fetch_config.toml`
```toml
redis_url = "redis://127.0.0.1:6379"
base_url  = "https://clob.polymarket.com"

node_id   = "fetch-001"
capacity_rps = 20
bind_addr = "0.0.0.0:3000"   # 监听地址
```

## 运行
- 启动 Redis
- 在每台 Fetch 主机运行：
```bash
./target/release/poly-ob-fetcher
```
- 在 Client 主机运行：
```bash
./target/release/poly-ob-client
```

## 通信协议（Client → Fetch）
- 传输：TCP，长度前缀 + JSON，Fetch 监听 `0.0.0.0:3000`
- 消息示例：
```json
{"tokens": ["id1", "id2", "id3"], "trigger": true}
```
- 若本次未携带 `tokens` 或为空，Fetch 使用上一次的 payload
- Fetch 收到后执行批量 `/books`，逐 token 原子更新 Redis，最后回写 "OK"

## 调度与限速
- 集群有 N 个 Fetch 节点，则全局时间片 Δ = 1/(20×N) 秒
- Client 每个时间片向各节点发送一批（一个 `/books` 请求），保证单节点 ≤ 20 req/s
- 批量大小 B 选择建议：令 `T×F ≤ B×20×N`（T 为 token 数，F 为目标刷新频率（次/秒））

## Redis 数据模型（仅保存最新快照）
- Key：`ob:{token_id}`（Hash）
  - `bids`：字符串（Polymarket 返回 JSON 原样）
  - `asks`：字符串（同上）
  - `hash`：字符串（订单簿哈希）
  - `timestamp`：字符串（订单簿时间戳，来自返回值）
  - `updated_at`：整数毫秒（Fetch 本地写入时间）
  - `market`：字符串（返回的 market id）
- 原子更新逻辑（Lua CAS）：
  - 若新 `hash == 当前 hash` → 跳过
  - 若新 `timestamp < 当前 timestamp` → 跳过
  - 否则覆盖写入上述字段（确保仅保留最新快照）

## 失败与恢复
- 4xx（payload 问题）记录并跳过；后续调度继续
- 5xx/网络错误：Fetch 本次失败，下一时间片 Client 继续调度
- Client 定期快速 TCP 连接探测节点可用性（200ms 超时）

## 扩展建议
- 持久化 Client→Fetch 长连接，减少握手开销（当前为短连接）
- 依据延迟/失败率自适应批量大小 B
- 节点自动注册/心跳与故障转移
- 可选引入 Redis Streams 记录审计流

## 参考
- Polymarket 文档（Books 接口）: https://docs.polymarket.com/developers/CLOB/prices-books/get-books