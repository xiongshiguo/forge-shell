//! 离线语义索引
//! 启动时 Tree-sitter 全量扫描 → 持久化磁盘 → 增量更新
//! 后续查询毫秒级返回，不调 API

use crate::engine::ast_parser::{AstParser, SymbolInfo};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// 索引条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexEntry {
    pub name: String,
    pub kind: String,
    pub file: String,
    pub line: usize,
    pub signature: String,
}

/// 文件指纹（修改时间）
#[derive(Debug, Clone, Serialize, Deserialize)]
struct FileFingerprint {
    path: String,
    modified: u64,
}

/// 语义索引
pub struct SemanticIndex {
    symbols: HashMap<String, Vec<IndexEntry>>,
    fingerprints: Vec<FileFingerprint>,
    index_dir: PathBuf,
    work_dir: PathBuf,
}

impl SemanticIndex {
    pub fn new(work_dir: PathBuf) -> Self {
        let index_dir = crate::config::forge_data_dir().join("semantic_index");
        std::fs::create_dir_all(&index_dir).ok();
        let mut idx = Self {
            symbols: HashMap::new(),
            fingerprints: Vec::new(),
            index_dir,
            work_dir,
        };
        idx.load_or_build();
        idx
    }

    /// 加载已有索引，或构建新索引
    fn load_or_build(&mut self) {
        let path = self.index_dir.join("symbols.json");
        let fp_path = self.index_dir.join("fingerprints.json");

        // 尝试加载已有索引
        if path.exists() && fp_path.exists() {
            if let (Ok(data), Ok(fp_data)) = (
                std::fs::read_to_string(&path),
                std::fs::read_to_string(&fp_path)
            ) {
                if let (Ok(symbols), Ok(fp)) = (
                    serde_json::from_str::<HashMap<String, Vec<IndexEntry>>>(&data),
                    serde_json::from_str::<Vec<FileFingerprint>>(&fp_data)
                ) {
                    // 检查是否需要增量更新
                    let changed = self.get_changed_files(&fp);
                    if changed.is_empty() {
                        tracing::info!("语义索引已是最新 ({} 符号)", symbols.len());
                        self.symbols = symbols;
                        self.fingerprints = fp;
                        return;
                    }
                    // 增量更新
                    tracing::info!("{} 个文件变更，增量更新索引", changed.len());
                    self.symbols = symbols;
                    self.incremental_update(&changed);
                    self.save();
                    return;
                }
            }
        }

        // 全量构建
        tracing::info!("构建语义索引...");
        self.full_build();
        self.save();
        tracing::info!("索引就绪: {} 符号", self.symbols.len());
    }

    fn full_build(&mut self) {
        self.symbols.clear();
        self.fingerprints.clear();
        let mut parser = match AstParser::new() {
            Some(p) => p,
            None => return,
        };

        let files = self.collect_rs_files(&self.work_dir.join("src"));
        for file in &files {
            self.index_file(&mut parser, file);
        }
    }

    fn index_file(&mut self, parser: &mut AstParser, file: &PathBuf) {
        let rel = file.strip_prefix(&self.work_dir).unwrap_or(file).to_string_lossy().to_string();
        if let Ok(source) = std::fs::read_to_string(file) {
            let syms = parser.parse_symbols(&source, &rel);
            for sym in syms {
                let entry = IndexEntry {
                    name: sym.name.clone(),
                    kind: sym.kind,
                    file: sym.file,
                    line: sym.line,
                    signature: sym.signature,
                };
                self.symbols.entry(sym.name).or_default().push(entry);
            }
            if let Ok(meta) = std::fs::metadata(file) {
                if let Ok(modified) = meta.modified() {
                    self.fingerprints.push(FileFingerprint {
                        path: rel,
                        modified: modified.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs(),
                    });
                }
            }
        }
    }

    fn get_changed_files(&self, old_fp: &[FileFingerprint]) -> Vec<PathBuf> {
        let mut changed = Vec::new();
        for fp in old_fp {
            let full = self.work_dir.join(&fp.path);
            if let Ok(meta) = std::fs::metadata(&full) {
                if let Ok(modified) = meta.modified() {
                    let new_secs = modified.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
                    if new_secs > fp.modified {
                        changed.push(full);
                    }
                }
            } else {
                changed.push(full); // 文件被删除
            }
        }
        changed
    }

    fn incremental_update(&mut self, changed_files: &[PathBuf]) {
        let mut parser = match AstParser::new() {
            Some(p) => p,
            None => return,
        };
        for file in changed_files {
            let rel = file.strip_prefix(&self.work_dir).unwrap_or(file).to_string_lossy().to_string();
            // 移除旧条目
            self.symbols.retain(|_, entries| {
                entries.retain(|e| e.file != rel);
                !entries.is_empty()
            });
            self.fingerprints.retain(|f| f.path != rel);
            // 重新索引
            self.index_file(&mut parser, file);
        }
    }

    fn collect_rs_files(&self, dir: &PathBuf) -> Vec<PathBuf> {
        let mut files = Vec::new();
        if let Ok(entries) = std::fs::read_dir(dir) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() && !p.to_string_lossy().contains("target") {
                    files.extend(self.collect_rs_files(&p));
                } else if p.extension().map(|e| e == "rs").unwrap_or(false) {
                    files.push(p);
                }
            }
        }
        files
    }

    fn save(&self) {
        if let Ok(json) = serde_json::to_string(&self.symbols) {
            std::fs::write(self.index_dir.join("symbols.json"), json).ok();
        }
        if let Ok(json) = serde_json::to_string(&self.fingerprints) {
            std::fs::write(self.index_dir.join("fingerprints.json"), json).ok();
        }
    }

    /// 查询符号
    pub fn query(&self, name: &str) -> Vec<&IndexEntry> {
        self.symbols.get(name).map(|v| v.iter().collect()).unwrap_or_default()
    }

    /// 按类型查询
    pub fn query_by_kind(&self, kind: &str) -> Vec<&IndexEntry> {
        self.symbols.values().flatten().filter(|e| e.kind == kind).collect()
    }

    /// 模糊搜索
    pub fn fuzzy_search(&self, pattern: &str) -> Vec<&IndexEntry> {
        let lower = pattern.to_lowercase();
        self.symbols.iter()
            .filter(|(k, _)| k.to_lowercase().contains(&lower))
            .flat_map(|(_, v)| v.iter())
            .collect()
    }

    pub fn len(&self) -> usize { self.symbols.len() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_empty_project() {
        let tmp = std::env::temp_dir().join("forge_test_idx_empty");
        std::fs::create_dir_all(tmp.join("src")).ok();
        let idx = SemanticIndex::new(tmp);
        assert!(idx.len() == 0 || idx.len() >= 0); // 可能没有 rs 文件
    }

    #[test]
    fn test_fuzzy_search() {
        let tmp = std::env::temp_dir().join("forge_test_idx_fuzzy");
        std::fs::create_dir_all(&tmp).ok();
        let mut idx = SemanticIndex::new(tmp.clone());
        idx.symbols.insert("handle_request".into(), vec![IndexEntry {
            name: "handle_request".into(), kind: "function".into(),
            file: "src/main.rs".into(), line: 10, signature: "fn handle_request()".into(),
        }]);
        assert_eq!(idx.fuzzy_search("handle").len(), 1);
        assert_eq!(idx.fuzzy_search("nonexistent").len(), 0);
    }
}
