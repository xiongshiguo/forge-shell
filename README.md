# 熔炉 (ForgeShell) — 以意为炉，以语为锤，铸代码之剑

> 面向中文开发者、成本极致、自我进化的下一代 AI 编程终端

[![构建状态](https://img.shields.io/badge/build-passing-brightgreen)]()
[![Rust](https://img.shields.io/badge/rust-1.85+-orange)]()
[![许可](https://img.shields.io/badge/license-MIT-blue)]()
[![平台](https://img.shields.io/badge/platform-Windows%20%7C%20Linux%20%7C%20鸿蒙-purple)]()

## 项目状态

🚧 **开发中** — 阶段 6/7 (性能优化与多平台编译)

- [x] 阶段 0：项目初始化与基础架构
- [x] 阶段 1：全中文 TUI 界面
- [x] 阶段 2：四级缓存引擎 + 并行调度器
- [x] 阶段 3：工具系统 + 红-绿-重构工作流
- [x] 阶段 4：项目监控面板 + 费用看板 + 社区大厅
- [x] 阶段 5：记忆系统 + 自动摘要 + 模型路由器
- [ ] 阶段 6：性能优化 + 多平台编译 + 文档完善
- [ ] 阶段 7：进化接口预留

## 快速开始

### 环境要求

- Rust 1.85+
- DeepSeek API Key（从 [DeepSeek 开放平台](https://platform.deepseek.com) 获取）

### 安装

```bash
# 1. 克隆项目
git clone <repo-url>
cd forge-shell

# 2. 编译（开发模式）
cargo build

# 3. 编译（发布模式，高性能）
cargo build --release

# 4. 设置 API Key
export DEEPSEEK_API_KEY="sk-your-key-here"

# 5. 启动
./target/release/forge-shell

# 6. 查看帮助
./target/release/forge-shell --help
```

### 多平台构建

```bash
# Windows / Linux / 鸿蒙
./build.sh all        # 全平台构建
./build.sh windows    # 仅 Windows
./build.sh linux      # 仅 Linux (Musl 静态编译)
./build.sh ohos       # 仅鸿蒙 (Linux 静态二进制)

# Windows 批处理
build.bat all
```

## 核心功能

### 三种工作模式

| 模式 | 快捷键 | 说明 |
|------|--------|------|
| 规划 | `Ctrl+P` | 只分析，不修改代码 |
| 助手 | `Ctrl+A` | 逐步执行，每步需确认 |
| 极速 | `Ctrl+Y` | 自动执行，事后汇总 |

### 四级缓存系统

```
L1: 系统提示词 (永久) → L2: 项目上下文 (跨会话) → L3: 会话缓存 (最近5轮) → L4: 挥发性
```
目标命中率 ≥97%，大幅降低 API 调用成本。

### 智能模型路由

- **简单任务** → DeepSeek Flash（成本仅为 Pro 的 10%）
- **复杂任务** → DeepSeek V4 Pro（精度优先）
- 自动评估意图复杂度，无需手动切换

### 并行任务调度

- 读优先调度：搜索、分析任务并行执行
- 依赖检测：写操作等待依赖完成
- 8-16 路并行，结果自动合并

### 红-绿-重构工作流

```
🔴 红 → 先写失败测试 → 🟢 绿 → 最小实现 → 🔵 重构 → 优化结构
```

### 安全沙箱

- 命令白名单机制
- 路径穿透检测
- 危险命令模式阻止
- 文件写入自动备份

## 快捷键一览

| 快捷键 | 功能 |
|--------|------|
| `Ctrl+P` | 切换到规划模式 |
| `Ctrl+A` | 切换到助手模式 |
| `Ctrl+Y` | 切换到极速模式 |
| `F1` | 项目监控面板 |
| `F2` | 费用看板 |
| `Ctrl+Shift+C` | 社区大厅 |
| `Ctrl+S` | 分享复盘 |
| `←` / `→` | 切换标签页 |
| `↑` / `↓` | 滚动对话 |
| `Ctrl+C` / `Esc` | 退出 |

## 项目结构

```
forge-shell/
├── src/
│   ├── main.rs             # 程序入口
│   ├── config.rs           # 配置管理
│   ├── error.rs            # 错误类型
│   ├── locale.rs           # 中文界面文本
│   ├── utils.rs            # 工具函数
│   ├── platform/           # 多平台适配
│   ├── tui/
│   │   ├── app.rs          # TUI 主应用
│   │   ├── keybindings.rs  # 快捷键
│   │   └── components/     # UI 组件
│   ├── agent/
│   │   ├── orchestrator.rs # 任务编排
│   │   ├── dispatcher.rs   # 任务分发
│   │   ├── workflow.rs     # 红-绿-重构
│   │   ├── tool_registry.rs# 工具注册
│   │   └── constraints.rs  # 约束检查
│   ├── engine/
│   │   ├── context/        # 四级缓存上下文
│   │   ├── cache/          # LRU 缓存
│   │   ├── inference/      # AI 推理客户端
│   │   ├── tools/          # 沙箱工具
│   │   ├── memory/         # 记忆系统
│   │   └── router.rs       # 模型路由器
│   └── evolution/          # 进化引擎 (预留)
├── .github/workflows/      # CI/CD
├── build.sh / build.bat    # 构建脚本
├── GOVERNANCE.md           # 治理章程
├── COMMUNITY.md            # 社区机制
└── README.md               # 本文件
```

## 技术栈

| 层次 | 技术 |
|------|------|
| 语言 | Rust 1.85+ |
| TUI | Ratatui + Crossterm |
| 异步 | Tokio |
| HTTP | Reqwest |
| 日志 | Tracing |
| AI 后端 | DeepSeek V4 Pro / Flash |

## 社区

熔炉是一个社区驱动的开源项目。

### 锻师会（治理）

- **大锻师**：最终决策权
- **锻师**：PR 审核、SOP 维护
- **学徒**：贡献者

详见 [GOVERNANCE.md](GOVERNANCE.md)

### 悬赏榜

任何人可发布悬赏任务推动项目发展：
- ¥500 以下免服务费
- ¥500 以上统一 5% 服务费

详见 [COMMUNITY.md](COMMUNITY.md)

### 加入贡献

1. Fork 项目
2. 提交 PR
3. 成为学徒 → 锻师

## 隐私承诺

- 复盘上传仅含脱敏策略
- 不上传任何代码、路径、变量名、API Key
- 分享弹窗明确列出不上传内容

## 费用优势

| 指标 | 熔炉 (DeepSeek) | Claude Code (Claude) | 节省 |
|------|-----------------|----------------------|------|
| 1M 输入 Token | ¥1 | ≈ ¥21 | 95%+ |
| 1M 输出 Token | ¥4 | ≈ ¥105 | 96%+ |
| 缓存命中 | 免费 | 半价 | - |

## 许可

MIT License — 详见 [LICENSE](LICENSE)
