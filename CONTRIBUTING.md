# 锻师会 · 贡献指南

## 锻师会是什么

锻师会是熔炉社区的核心治理机制——贡献者通过代码、文档、SOP 策略等贡献成长为锻师，参与项目决策。

### 三级晋升

| 等级 | 条件 | 权益 |
|------|------|------|
| **学徒** | 任意 PR 被合入 | Gitee Contributor 标签，进入 CONTRIBUTORS.md |
| **锻师** | 3+ PR 合入，含至少 1 个功能特性 | Gitee 仓库写权限，悬赏榜认领优先权 |
| **大锻师** | 10+ PR 合入，持续 2 月以上 | 核心维护者，代码审核权，TAG 签名权 |

晋升由现有大锻师投票，每两周评定一次。

## 快速开始

```bash
# 1. 克隆仓库
git clone https://gitee.com/forgemaster/forge-shell.git
cd forge-shell

# 2. 安装 Rust
# https://rustup.rs

# 3. 构建
cargo build --release

# 4. 配置 API Key
echo 'DEEPSEEK_API_KEY=sk-xxx' > .env

# 5. 运行
./target/release/forge-shell.exe

# 6. 运行测试
cargo test
```

## 项目结构

```
src/
├── main.rs          # 入口，CLI 解析
├── config.rs        # 配置管理
├── system_prompt.rs # AI 系统提示词
├── error_log.rs     # 错误日志系统
├── web/
│   ├── mod.rs       # Axum 路由 + AppState
│   └── api.rs       # 全部 API 处理器
├── engine/
│   ├── inference/   # DeepSeek API 客户端 + 消息类型
│   ├── router.rs    # 模型路由器
│   ├── cache/       # 四级缓存系统
│   ├── context/     # 上下文管理器
│   ├── prompt_optimizer.rs  # UCB1 提示词优化器
│   ├── semantic_index.rs    # Tree-sitter 语义索引
│   ├── ast_parser.rs        # AST 解析器
│   ├── mcp.rs       # MCP 协议支持
│   └── tools/
│       ├── sandbox.rs  # 沙箱
│       └── backup.rs   # 备份管理
├── agent/
│   ├── agent_executor.rs  # 子 Agent 执行器
│   ├── orchestrator.rs    # 任务编排器
│   └── dispatcher.rs      # 任务分发器
├── evolution/
│   ├── mod.rs         # 进化协调器
│   ├── collector.rs   # 经验采集器
│   ├── reflection.rs  # 反思引擎
│   └── sop.rs         # SOP 库
├── tui/                # TUI 界面
└── locale/             # 国际化
assets/web/             # Web UI 静态文件
```

## 提 PR 规范

1. **commit 格式**：`vX.Y.Z: 简短描述`，Co-Authored-By 你的名字
2. **测试要求**：新增功能需带测试，`cargo test` 必须全过
3. **代码风格**：跟现有代码一致，不要大改格式
4. **PR 描述**：写清楚 WHY（为什么改）和 HOW（怎么改的）
5. **签 CLA**：首次 PR 需同意 AGPLv3 贡献协议

## 开发环境

- Rust 1.85+ (edition 2024)
- DeepSeek API Key（测试用，可设环境变量）
- Windows/Linux/macOS 均可
- IDE：VS Code + rust-analyzer（推荐）

## 悬赏榜

查看 [悬赏榜.md](./悬赏榜.md) 获取当前待认领任务。认领方式：在对应 Gitee Issue 下回复"认领"。

## 联系方式

- 社区官网：https://forgeshell.cn
- Gitee Issues：https://gitee.com/forgemaster/forge-shell/issues
- 代码仓库：https://gitee.com/forgemaster/forge-shell
