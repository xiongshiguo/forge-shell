# 熔炉 (ForgeShell) — 以意为炉，以语为锤，铸代码之剑

## 项目状态

🚧 开发中（阶段 0 完成 → 阶段 1 进行中）

## 快速开始

```bash
# 1. 设置 API Key
export DEEPSEEK_API_KEY="your-key"

# 2. 编译运行
cargo build --release
./target/release/forge-shell

# 3. 查看帮助
./target/release/forge-shell --help
```

## 快捷键

| 快捷键 | 功能 |
|--------|------|
| Ctrl+P | 规划模式（只分析不修改） |
| Ctrl+A | 助手模式（逐步确认） |
| Ctrl+Y | 极速模式（自动执行） |
| F1 | 项目监控面板 |
| F2 | 费用看板 |
| Ctrl+Shift+C | 社区大厅 |
| Ctrl+S | 分享复盘 |

## 分阶段开发

- [x] 阶段 0：项目初始化、目录结构、基础配置
- [ ] 阶段 1：全中文 TUI 界面
- [ ] 阶段 2：四级缓存 + 并行调度
- [ ] 阶段 3：工具系统 + 工作流
- [ ] 阶段 4：监控面板 + 社区功能
- [ ] 阶段 5：记忆系统 + 路由器
- [ ] 阶段 6：性能优化 + 多平台
- [ ] 阶段 7：进化接口

## 技术栈

- Rust 1.85+
- Ratatui (TUI)
- Tokio (异步)
- Reqwest (HTTP)
- DeepSeek V4 Pro / Flash

## 许可

MIT
