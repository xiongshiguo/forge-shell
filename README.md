# 熔炉 (ForgeShell)

> 面向中文开发者、成本极致、自我进化的下一代 AI 编程终端

[![版本](https://img.shields.io/badge/version-0.21.41-blue)]()
[![Rust](https://img.shields.io/badge/rust-1.85+-orange)]()
[![许可](https://img.shields.io/badge/license-MIT-blue)]()
[![平台](https://img.shields.io/badge/platform-Windows%20%7C%20Linux%20%7C%20macOS-purple)]()

## 项目简介

熔炉 (ForgeShell) 是一款基于 DeepSeek V4 API 的 AI 编程终端。以"意为炉，以语为锤，铸代码之剑"为理念，提供 Web UI，支持模型路由、工具调用、思考分离、跨会话记忆和社区进化等核心能力。

## 核心特性

| 特性 | 说明 |
|------|------|
| **模型路由** | Simple/Moderate → Flash (快速), Complex → Pro (深度推理+思考模式) |
| **思考/回答分离** | Pro 思考过程在可折叠区块中独立展示，与最终回答物理隔离 |
| **16 个工具** | read/write/edit/search/glob/exec/lsp/web/web-fetch/todo/ask/semantic/snap/save/community-fix/share-fix |
| **工具智能筛选** | Flash 根据用户意图关键词动态精简工具列表（16→≤10） |
| **Pro 降级保护** | Pro 45s 无响应自动切 Flash，Complex 任务永不卡死 |
| **流式超时保护** | 无内容 45s + 总流 150s 双重超时，keepalive 帧无法续命 |
| **跨会话记忆** | 自动保存/加载会话，支持最近会话列表和恢复 |
| **版本检测** | Tags API + semver 比较，自动提示更新 |

## 快速开始

### 环境要求

- Rust 1.85+
- DeepSeek API Key ([获取地址](https://platform.deepseek.com))

### 安装构建

```bash
git clone https://gitee.com/forgemaster/forge-shell
cd forge-shell
cargo build --release
```

### 启动运行

```bash
# 启动 Web UI（默认，自动打开浏览器）
./target/release/forge-shell

# 命令行传入 API Key
./target/release/forge-shell --key sk-your-api-key

# 启动 TUI 终端模式
./target/release/forge-shell --tui
```

首次启动后在 Web UI 输入 API Key，之后自动保存，无需重复输入。

## 工作模式

| 模式 | 说明 | 模型 | 工具限制 |
|------|------|------|---------|
| **助手** (assist) | 默认模式 | 路由自动选择 | 全部 |
| **规划** (plan) | 只分析不修改 | 强制 Pro | 只读 |
| **极速** (speed) | 最快响应 | 强制 Flash | 全部 |

模型偏好：**智能**(auto) / **Pro** / **Flash** / **本地**(Ollama)

## API 接口

| 端点 | 方法 | 说明 |
|------|------|------|
| /api/chat | POST | 流式对话 (SSE) |
| /api/check-key | GET | 检查 API Key |
| /api/setup | POST | 初始化配置 |
| /api/update-check | GET | 版本更新检查 |
| /api/status | GET | 获取运行状态 |
| /api/cost | GET | 获取费用统计 |
| /api/session/latest | GET | 获取最近会话 |
| /api/sessions | GET | 获取会话列表 |
| /api/session/auto-save | POST | 自动保存会话 |
| /api/logs | GET | 错误日志查询 |

## 项目结构

```
forge-shell/
├── src/
│   ├── main.rs              # 入口 + PID 管理 + 日志
│   ├── config.rs            # 配置管理
│   ├── system_prompt.rs     # AI 系统提示词
│   ├── error.rs             # 错误类型
│   ├── error_log.rs         # 错误日志（环形缓冲+持久化）
│   ├── engine/
│   │   ├── inference/       # DeepSeek API 调用 + SSE 解析
│   │   ├── router.rs        # 模型路由（Simple/Moderate/Complex）
│   │   ├── stream.rs        # StreamAccumulator（流式累积器）
│   │   ├── session.rs       # SessionManager（原子保存/加载）
│   │   ├── conversation.rs  # Conversation 构建器（类型安全）
│   │   └── tools/           # 工具执行 + 沙箱
│   ├── web/
│   │   ├── api.rs           # 主要 API 处理器 + 工具定义执行
│   │   └── static_files.rs  # 嵌入静态资源
│   ├── agent/
│   │   ├── orchestrator.rs  # 任务分解
│   │   └── agent_executor.rs # 子任务执行
│   └── evolution/           # 进化引擎
├── assets/web/              # 前端资源（HTML/CSS/JS）
└── site/                    # 文档站点
```

## 费用优势

| 指标 | 熔炉 (DeepSeek V4) | Claude Code |
|------|---------------------|-------------|
| Flash 输入/输出 | ¥0.1/M / ¥0.4/M | **200 倍** |
| Pro 输入/输出 | ¥1/M / ¥4/M | **20 倍** |

## 许可证

MIT 许可证，详见 [LICENSE](./LICENSE)。

## 相关链接

- **官网**: https://forgeshell.cn
- **仓库**: https://gitee.com/forgemaster/forge-shell
- **DeepSeek API**: https://platform.deepseek.com
- **DeepSeek 思考模式文档**: https://api-docs.deepseek.com/zh-cn/guides/thinking_mode
