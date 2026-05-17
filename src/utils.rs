/// 截断字符串至指定字符数
pub fn truncate(s: &str, max_chars: usize) -> &str {
    let end = s.char_indices()
        .take(max_chars)
        .last()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    &s[..end]
}

/// 截断并在末尾加省略号
pub fn truncate_with_ellipsis(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let mut truncated = truncate(s, max_chars.saturating_sub(1)).to_string();
        truncated.push('…');
        truncated
    }
}

/// 格式化字节数为人类可读格式
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;
    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }
    if unit_idx == 0 {
        format!("{} {}", bytes, UNITS[unit_idx])
    } else {
        format!("{:.1} {}", size, UNITS[unit_idx])
    }
}

/// 格式化金额（人民币）
pub fn format_cost(yuan: f64) -> String {
    if yuan < 0.0001 {
        "¥0".to_string()
    } else if yuan < 0.01 {
        format!("¥{:.6}", yuan)
    } else if yuan < 1.0 {
        format!("¥{:.4}", yuan)
    } else {
        format!("¥{:.2}", yuan)
    }
}

/// 格式化百分比
pub fn format_percent(rate: f64) -> String {
    format!("{:.0}%", rate * 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("hello", 3), "hel");
        assert_eq!(truncate("你好世界", 2), "你好");
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1048576), "1.0 MB");
    }

    #[test]
    fn test_format_cost() {
        assert_eq!(format_cost(0.0), "¥0");
        assert_eq!(format_cost(0.0032), "¥0.003200");
        assert_eq!(format_cost(1.5), "¥1.50");
    }
}
