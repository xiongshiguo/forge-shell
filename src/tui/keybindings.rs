//! 快捷键绑定

use crossterm::event::{KeyCode, KeyModifiers};

/// 快捷键事件
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyEvent {
    /// 切换到规划模式
    ModePlan,
    /// 切换到助手模式
    ModeAssist,
    /// 切换到极速模式
    ModeSpeed,
    /// 打开项目监控
    ProjectPanel,
    /// 打开费用看板
    CostPanel,
    /// 打开社区大厅
    CommunityPanel,
    /// 分享复盘
    ShareReview,
    /// 退出
    Quit,
    /// 普通输入
    Char(char),
    /// 回车
    Enter,
    /// 退格
    Backspace,
    /// 上/下 滚动
    ScrollUp,
    ScrollDown,
}

/// 将 crossterm 按键转换为熔炉按键事件
pub fn map_key(key: crossterm::event::KeyEvent) -> Option<KeyEvent> {
    match key {
        // Ctrl+P → 规划模式
        crossterm::event::KeyEvent {
            code: KeyCode::Char('p'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => Some(KeyEvent::ModePlan),
        // Ctrl+A → 助手模式
        crossterm::event::KeyEvent {
            code: KeyCode::Char('a'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => Some(KeyEvent::ModeAssist),
        // Ctrl+Y → 极速模式
        crossterm::event::KeyEvent {
            code: KeyCode::Char('y'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => Some(KeyEvent::ModeSpeed),
        // F1 → 项目监控
        crossterm::event::KeyEvent {
            code: KeyCode::F(1),
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(KeyEvent::ProjectPanel),
        // F2 → 费用看板
        crossterm::event::KeyEvent {
            code: KeyCode::F(2),
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(KeyEvent::CostPanel),
        // Ctrl+Shift+C → 社区大厅
        crossterm::event::KeyEvent {
            code: KeyCode::Char('C'),
            modifiers: KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            ..
        } => Some(KeyEvent::CommunityPanel),
        // Ctrl+S → 分享复盘
        crossterm::event::KeyEvent {
            code: KeyCode::Char('s'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => Some(KeyEvent::ShareReview),
        // Ctrl+C → 退出
        crossterm::event::KeyEvent {
            code: KeyCode::Char('c'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => Some(KeyEvent::Quit),
        // Esc → 退出
        crossterm::event::KeyEvent {
            code: KeyCode::Esc,
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(KeyEvent::Quit),
        // Enter
        crossterm::event::KeyEvent {
            code: KeyCode::Enter,
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(KeyEvent::Enter),
        // Backspace
        crossterm::event::KeyEvent {
            code: KeyCode::Backspace,
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(KeyEvent::Backspace),
        // 字符输入
        crossterm::event::KeyEvent {
            code: KeyCode::Char(c),
            modifiers: KeyModifiers::NONE | KeyModifiers::SHIFT,
            ..
        } => Some(KeyEvent::Char(c)),
        // 上箭头
        crossterm::event::KeyEvent {
            code: KeyCode::Up,
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(KeyEvent::ScrollUp),
        // 下箭头
        crossterm::event::KeyEvent {
            code: KeyCode::Down,
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(KeyEvent::ScrollDown),
        _ => None,
    }
}
