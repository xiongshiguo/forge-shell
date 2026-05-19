

# 熔炉 (ForgeShell)

> 面向中文开发者、成本极致、自我进化的下一代 AI 编程终端

[![版本](https://img.shields.io/badge/version-0.4.1-orange)]()
[![Rust](https://img.shields.io/badge/rust-1.85+-orange)]()
[![许可](https://img.shields.io/badge/license-MIT-blue)]()
[![平台](https://img.shields.io/badge/platform-Windows%20%7C%20Linux%20%7C%20鸿蒙-purple)]()

## 项目简介

熔炉 (ForgeShell) 是一款基于 DeepSeek API 的 AI 编程终端，以「意为炉，以语为锤，铸代码之剑」为理念。它同时提供 Web UI 和 TUI 两种界面，支持四级缓存、模型路由、沙箱执行和跨会话记忆等核心功能，旨在为中国开发者提供低成本、高效率的智能编程体验。

## 核心特性

熔炉具备以下核心特性：

1. **双界面模式**：Web UI 运行在 localhost:9527，采用紫金配色，支持 SSE 流式对话；TUI 提供全中文终端界面，支持三种工作模式（规划/助手/极速）

2. **四级缓存系统**：通过 `src/engine/cache/mod.rs` 实现目标命中率 ≥97% 的四级缓存机制，大幅降低 API 调用成本

3. **智能模型路由**：通过 `src/engine/router.rs` 根据意图复杂度动态选择 Flash（低成本）或 Pro（高精度）模型，自动优化费用

4. **沙箱命令执行**：通过 `src/engine/tools/sandbox.rs` 实现白名单命令（cargo check/test/build）执行保护，实时返回 stdout/stderr

5. **进化引擎**：通过 `src/evolution/mod.rs` 实现「经验采集 → 反思提炼 → SOP 匹配」的闭环进化机制

6. **跨会话记忆**：自动加载/保存 FORGESHELL_CONTEXT.md，关闭后重新打开仍可继续之前的上下文

7. **版本检测**：启动时自动检查最新 Release 版本，提示用户更新

## 快速开始

### 安装构建

```bash
# 克隆仓库
git clone https://gitee.com/forgemaster/forge-shell
cd forge-shell

# 构建项目
cargo build --release

# 或者使用构建脚本
./build.sh          # Linux
.\build.bat         # Windows
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

### 首次配置

首次启动后，输入 DeepSeek API Key（可从 [DeepSeek 开放平台](https://platform.deepseek.com) 获取），之后程序会自动保存密钥，无需重复输入。

## 项目结构

```
forge-shell/
├── src/
│   ├── main.rs              # 程序入口
│   ├── config.rs            # 配置管理
│   ├── system_prompt.rs     # AI 人格定义
│   ├── error.rs             # 错误类型定义
│   ├── locale.rs            # 国际化支持
│   ├── utils.rs             # 工具函数
│   ├── platform.rs          # 平台适配
│   ├── tui/                 # TUI 终端界面
│   │   ├── app.rs           # 应用主逻辑
│   │   ├── keybindings.rs   # 快捷键映射
│   │   └── components/      # UI 组件
│   ├── web/                 # Web UI 服务
│   │   ├── api.rs           # API 处理器
│   │   └── static_files.rs  # 静态资源
│   ├── agent/               # 任务编排
│   │   ├── orchestrator.rs  # 任务分解
│   │   ├── dispatcher.rs    # 任务分发
│   │   ├── workflow.rs      # 工作流管理
│   │   ├── tool_registry.rs # 工具注册
│   │   └── constraints.rs   # 约束检查
│   ├── engine/              # 核心引擎
│   │   ├── cache/           # 四级缓存
│   │   ├── context/         # 上下文管理
│   │   ├── inference/       # API 调用
│   │   ├── memory/          # 记忆管理
│   │   ├── tools/           # 工具执行
│   │   └── router.rs        # 模型路由
│   └── evolution/           # 进化引擎
│       ├── collector.rs      # 经验采集
│       ├── reflection.rs   # 反思引擎
│       └── sop.rs          # SOP 库
├── assets/web/              # Web 前端资源
└── site/                    # 文档站点
```

## 技术栈

| 层次 | 技术选型 |
|------|----------|
| 编程语言 | Rust 1.85+ |
| TUI 框架 | Ratatui + Crossterm |
| Web 框架 | Axum + Rust-Embed |
| AI 服务 | DeepSeek API (deepseek-chat) |
| 前端 | 纯 HTML/CSS/JS，无框架依赖 |

## 快捷键说明（TUI 模式）

| 快捷键 | 功能说明 |
|--------|----------|
| Ctrl+P | 切换到规划模式 |
| Ctrl+A | 切换到助手模式 |
| Ctrl+Y | 切换到极速模式 |
| Ctrl+S | 分享当前对话 |
| Ctrl+C | 退出程序 |
| F1 | 打开项目监控面板 |
| F2 | 打开费用看板面板 |
| ← / → | 切换侧边栏标签 |

## API 接口

熔炉提供以下 Web API 端点：

| 端点 | 方法 | 说明 |
|------|------|------|
| /api/chat | POST | 流式对话接口 |
| /api/setup | POST | 初始化配置 |
| /api/status | GET | 获取状态信息 |
| /api/cost | GET | 获取费用统计 |
| /api/project | GET | 获取项目信息 |
| /api/exec | POST | 执行测试命令 |
| /api/ping | GET | 健康检查 |

## 费用优势

相比 Claude Code，熔炉在成本方面具有显著优势：

| 指标 | 熔炉 (DeepSeek) | Claude Code | 节省比例 |
|------|-----------------|-------------|----------|
| 1M Token 输入 | ¥1 | ≈ ¥21 | 95%+ |
| 内存占用 | ~15 MB | 300+ MB | - |
| 缓存命中 | 免费 | 半价 | - |

## 配置文件

熔炉的配置文件位于用户数据目录，配置结构如下：

```json
{
  "ai": {
    "api_key": "your-key",
    "model": "deepseek-chat",
    "flash_model": "deepseek-flash",
    "api_base": "https://api.deepseek.com"
  },
  "ui": {
    "mode": "web",
    "scrollback": 10000
  },
  "engine": {
    "parallel": 4,
    "cache_target": 0.97,
    "session_rounds": 100
  }
}
```

## 社区参与

欢迎开发者参与熔炉项目的贡献！您可以：

- 提交 Issue 报告问题或建议
- 提交 Pull Request 贡献代码
- 参与社区讨论，详见 [COMMUNITY.md](./COMMUNITY.md)
- 了解治理机制，详见 [GOVERNANCE.md](./GOVERNANCE.md)

## 许可证

本项目基于 MIT 许可证开源，详见 [LICENSE](./LICENSE) 文件。

## 相关链接

- **官网**: https://forgeshell.cn
- **仓库**: https://gitee.com/forgemaster/forge-shell
- **DeepSeek API**: https://platform.deepseek.com