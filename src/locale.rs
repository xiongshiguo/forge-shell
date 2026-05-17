/// 界面文本集中管理（当前仅中文）
/// 预留多语言接口，所有 UI 文本统一从此处获取

use std::collections::HashMap;

/// 语言类型（预留）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    ZhCN,
    // EnUS, // 预留
}

impl Default for Lang {
    fn default() -> Self {
        Lang::ZhCN
    }
}

/// 文本管理器
pub struct Locale {
    lang: Lang,
    texts: HashMap<&'static str, &'static str>,
}

impl Default for Locale {
    fn default() -> Self {
        Self::new(Lang::ZhCN)
    }
}

impl Locale {
    pub fn new(lang: Lang) -> Self {
        let texts = match lang {
            Lang::ZhCN => zh_cn_texts(),
        };
        Self { lang, texts }
    }

    pub fn get(&self, key: &str) -> String {
        self.texts
            .get(key)
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("[{}]", key))
    }
}

fn zh_cn_texts() -> HashMap<&'static str, &'static str> {
    let mut m = HashMap::new();

    // 模式名称
    m.insert("mode.plan", "规划");
    m.insert("mode.assist", "助手");
    m.insert("mode.speed", "极速");

    // 模式描述
    m.insert("mode.plan.desc", "只分析，不修改");
    m.insert("mode.assist.desc", "逐步执行，需确认");
    m.insert("mode.speed.desc", "自动执行，事后汇总");

    // 快捷键
    m.insert("key.plan", "Ctrl+P");
    m.insert("key.assist", "Ctrl+A");
    m.insert("key.speed", "Ctrl+Y");
    m.insert("key.project", "F1");
    m.insert("key.cost", "F2");
    m.insert("key.community", "Ctrl+Shift+C");
    m.insert("key.share", "Ctrl+S");
    m.insert("key.quit", "Ctrl+C");

    // 面板标题
    m.insert("panel.project", "项目监控");
    m.insert("panel.cost", "费用看板");
    m.insert("panel.community", "社区大厅");
    m.insert("panel.chat", "对话");

    // 状态栏
    m.insert("status.ready", "就绪");
    m.insert("status.thinking", "思考中…");
    m.insert("status.executing", "执行中…");
    m.insert("status.waiting", "等待确认");

    // 社区
    m.insert("community.pool", "经验熔池");
    m.insert("community.sop", "天工阁");
    m.insert("community.bounty", "悬赏榜");
    m.insert("community.forge", "锻师会");

    // 按钮
    m.insert("btn.confirm", "确认");
    m.insert("btn.cancel", "取消");
    m.insert("btn.retry", "重试");
    m.insert("btn.skip", "跳过");
    m.insert("btn.share", "分享复盘");

    // 提示
    m.insert("hint.input", "输入你的指令…");
    m.insert("hint.search", "搜索…");

    // 错误
    m.insert("error.api_key", "未设置 DEEPSEEK_API_KEY 环境变量");
    m.insert("error.network", "网络连接失败，请检查网络");
    m.insert("error.timeout", "请求超时");

    // 应用标题
    m.insert("app.title", "熔炉 (ForgeShell)");
    m.insert("app.slogan", "以意为炉，以语为锤，铸代码之剑");

    m
}

/// 便捷函数：获取中文文本
pub fn t(key: &str) -> String {
    Locale::default().get(key)
}
