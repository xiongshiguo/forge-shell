# 熔炉 (ForgeShell) — 以意为炉，以语为锤，铸代码之剑

> 面向中文开发者、成本极致、自我进化的下一代 AI 编程终端

[![版本](https://img.shields.io/badge/version-0.4.1-orange)]()
[![Rust](https://img.shields.io/badge/rust-1.85+-orange)]()
[![许可](https://img.shields.io/badge/license-MIT-blue)]()
[![平台](https://img.shields.io/badge/platform-Windows%20%7C%20Linux%20%7C%20鸿蒙-purple)]()
[![测试](https://img.shields.io/badge/tests-36%20passed-green)]()

## 项目状态

⚡ **V2.3 能力觉醒版** — 模型路由 + 沙箱执行 + 跨会话记忆

- [x] 阶段 0-7：基础架构、TUI/Web UI、四级缓存、工具沙箱、进化引擎
- [x] V2 Web UI：紫金配色浏览器界面，SSE 流式对话
- [x] V2.1：真实 DeepSeek API，Key 持久化，版本检测
- [x] V2.2：进化闭环（经验采集 → 反思 → SOP 匹配）
- [x] V2.3：模型路由（Flash/Pro 动态选择）、沙箱命令执行、跨会话记忆

## 快速开始

```bash
# 启动 Web UI（默认，自动打开浏览器）
./forge-shell

# 首次使用输入 DeepSeek API Key，之后自动记住

# 命令行传 Key
./forge-shell --key sk-你的key

# TUI 终端模式
./forge-shell --tui
```

## 核心功能

| 功能 | 说明 |
|------|------|
| 🌐 Web UI | localhost:9527，紫金配色，SSE 流式对话 |
| 💻 TUI | 全中文终端界面，三种模式 (Ctrl+P/A/Y) |
| 📊 四级缓存 | 目标命中率 ≥97%，大幅降低 API 成本 |
| 🧠 模型路由 | 按意图复杂度动态选 Flash（便宜）或 Pro（准确），自动省钱 |
| 🛡️ 沙箱执行 | cargo check/test/build 白名单命令，实时返回 stdout/stderr |
| 🔄 进化引擎 | 每次对话匿名采集 → 反思提炼 SOP → 智能匹配 |
| 💾 跨会话记忆 | FORGESHELL_CONTEXT.md 自动加载/保存，关掉重开还记得 |
| 🔔 版本检测 | 启动时自动查最新 Release，提示下载 |
| 🔧 测试链 | /api/exec 端点，AI 可调用执行测试并报告结果 |

## 快捷键（TUI）

| 键 | 功能 | 键 | 功能 |
|----|------|----|------|
| Ctrl+P | 规划模式 | F1 | 项目监控 |
| Ctrl+A | 助手模式 | F2 | 费用看板 |
| Ctrl+Y | 极速模式 | Ctrl+S | 分享复盘 |
| Ctrl+C | 退出 | ←→ | 切换标签 |

## 项目结构

```
forge-shell/
├── src/
│   ├── main.rs             # 程序入口
│   ├── system_prompt.rs    # AI 人格定义
│   ├── config.rs           # 配置管理
│   ├── tui/                # TUI 终端界面
│   ├── web/                # Web UI (axum + rust-embed)
│   ├── agent/              # 任务编排 + 分发 + 工作流
│   ├── engine/             # 缓存/推理/工具/记忆/路由
│   └── evolution/          # 进化引擎（采集/反思/SOP）
├── assets/web/             # Web 前端静态文件
└── site/                   # 社区网站 (VitePress)
```

## 技术栈

| 层次 | 技术 |
|------|------|
| 语言 | Rust 1.85+ |
| TUI | Ratatui + Crossterm |
| Web | Axum + rust-embed |
| AI | DeepSeek API (deepseek-chat) |
| 前端 | 纯 HTML/CSS/JS，无框架 |

## 社区

- **官网**：https://forgeshell.cn
- **仓库**：https://gitee.com/forgemaster/forge-shell
- **治理**：锻师会（大锻师→锻师→学徒）
- **悬赏榜**：发布任务推动项目发展
- **天工阁**：社区 SOP 库

## 费用优势

| 指标 | 熔炉 (DeepSeek) | Claude Code | 节省 |
|------|-----------------|-------------|------|
| 1M Token 输入 | ¥1 | ≈ ¥21 | 95%+ |
| 内存占用 | 15 MB | 300+ MB | - |
| 缓存命中 | 免费 | 半价 | - |

## License

MIT License
