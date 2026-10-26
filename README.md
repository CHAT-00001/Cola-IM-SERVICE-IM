# 🚀 High-Performance IM Server

一个 **面向千万级并发、支持多协议、多聊天形态、端到端加密** 的新一代即时通讯（IM）服务器。

适用于：社交 IM、实时对战游戏、直播互动、企业通讯、内容社区等场景。

---

## ✨ 核心特性

- 单用户多端多设备同时在线
- 单机最高支持 500 万在线连接
- 水平无限扩展（无状态架构）
- 端到端加密（E2EE）
- 游戏战斗级低延迟动作信令
- 独立 IM Gateway 架构

---

## 🛠 技术栈

- **语言**：Rust
- **异步运行时**：Tokio
- **数据库**：MongoDB
- **配置文件**：application.toml
- **启动命令**：cargo run

---

## 🌐 协议支持

- WebSocket（标准 IM）
- WebRTC（音视频 / P2P）
- 私有 UDP 协议
- 游戏级实时动作信令协议

---

## ▶️ 启动

```bash
cargo run


📜 License

MIT License


---

# 📄 ARCHITECTURE.md（架构说明）

```md
# 🧩 IM Server Architecture

## 🏗 总体架构

系统采用 **Gateway + Node 集群** 的扁平化架构设计。

```text
Client
  │
  ▼
IM Gateway Cluster
  │
  ├── IM Node Cluster
  ├── RTC Service
  ├── Game Realtime Service
  └── Push Service
🚪 IM Gateway
职责

长连接统一接入

多协议接入（WS / UDP / WebRTC）

消息路由与转发

负载均衡

连接鉴权

特点

无状态设计

可水平扩展

支持百万级连接扇出

🧠 IM Node
功能模块

会话管理（Session）

多端多设备同步

消息投递与确认

消息状态管理

离线消息处理

🎮 实时对战子系统

基于 UDP

不保证可靠但保证低延迟

适用于：

游戏动作同步

实时对抗

协作编辑

🔐 安全模型

端到端加密（E2EE）

Gateway 不解密消息内容

公共频道支持内容治理


---

# 📄 PROTOCOL.md（协议说明）

```md
# 📡 Communication Protocol

## 📨 消息模型

```json
{
  "message_id": "uuid",
  "chat_type": "single | group | public | push | realtime",
  "from": "user_id",
  "to": "target_id",
  "message_type": "text | image | video | file | action",
  "payload": "encrypted_data",
  "timestamp": 1700000000
}

💬 聊天模式

单聊（P2P）

群聊

公共频道

系统推送

实时对战 / 动作信令

🧠 消息能力

撤回

阅后即焚

收藏

引用 / 回复

多端同步

🎮 实时动作信令

基于 UDP

消息体极简

无持久化

高吞吐、低延迟


---

# 📄 DATABASE.md（MongoDB 设计）

```md
# 🗄 Database Design (MongoDB)

## 📦 Collections

---

## 1️⃣ message

消息主表（仅存元数据）

- _id
- message_id
- chat_type
- message_type
- from
- to
- timestamp
- status

---

## 2️⃣ inbox

收件箱

- user_id
- message_id
- read_status
- burned
- referenced_message_id

---

## 3️⃣ outbox

发件箱

- user_id
- message_id
- delivered
- synced_devices

---

## 📈 索引策略

- message_id（唯一）
- user_id + timestamp
- chat_type + to