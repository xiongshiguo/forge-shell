//! Tree-sitter Rust AST 解析器
//! 提供精确的符号定义、引用、类型信息

use tree_sitter::{Parser, Query, QueryCursor};

/// 符号信息
#[derive(Debug, Clone)]
pub struct SymbolInfo {
    pub name: String,
    pub kind: String, // function/struct/enum/trait/impl/module
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub signature: String,
}

/// 引用信息
#[derive(Debug, Clone)]
pub struct ReferenceInfo {
    pub symbol: String,
    pub file: String,
    pub line: usize,
    pub context: String,
}

/// AST 解析器
pub struct AstParser {
    parser: Parser,
    rust_lang: tree_sitter::Language,
}

impl AstParser {
    pub fn new() -> Option<Self> {
        let rust_lang = tree_sitter_rust::LANGUAGE.into();
        let mut parser = Parser::new();
        parser.set_language(&rust_lang).ok()?;
        Some(Self { parser, rust_lang })
    }

    /// 解析文件获取所有符号定义
    pub fn parse_symbols(&mut self, source: &str, file_path: &str) -> Vec<SymbolInfo> {
        let tree = match self.parser.parse(source, None) {
            Some(t) => t,
            None => return vec![],
        };

        let mut symbols = Vec::new();
        let root = tree.root_node();
        self.collect_symbols(root, source, file_path, &mut symbols);
        symbols
    }

    fn collect_symbols(&self, node: tree_sitter::Node, source: &str, file: &str, symbols: &mut Vec<SymbolInfo>) {
        let kind = node.kind();
        match kind {
            "function_item" | "function_signature_item" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = name_node.utf8_text(source.as_bytes()).unwrap_or("");
                    let start = node.start_position();
                    let sig = &source[node.start_byte()..node.end_byte().min(node.start_byte() + 120)];
                    symbols.push(SymbolInfo {
                        name: name.to_string(), kind: "function".into(),
                        file: file.into(), line: start.row + 1, column: start.column + 1,
                        signature: sig.chars().take(100).collect(),
                    });
                }
            }
            "struct_item" | "enum_item" | "trait_item" | "impl_item" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = name_node.utf8_text(source.as_bytes()).unwrap_or("");
                    let start = node.start_position();
                    let kind_str = kind.trim_end_matches("_item");
                    symbols.push(SymbolInfo {
                        name: name.to_string(), kind: kind_str.into(),
                        file: file.into(), line: start.row + 1, column: start.column + 1,
                        signature: String::new(),
                    });
                }
            }
            _ => {}
        }

        for child in node.children(&mut node.walk()) {
            self.collect_symbols(child, source, file, symbols);
        }
    }

    /// 搜索符号的所有引用（通过同名文本搜索）
    pub fn find_references(&self, source: &str, symbol: &str, file: &str) -> Vec<ReferenceInfo> {
        let mut refs = Vec::new();
        let lines: Vec<&str> = source.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            if line.contains(symbol) {
                refs.push(ReferenceInfo {
                    symbol: symbol.into(),
                    file: file.into(),
                    line: i + 1,
                    context: line.trim().chars().take(80).collect(),
                });
            }
        }
        refs
    }

    /// 获取文件中的 use/import 语句
    pub fn parse_imports(&mut self, source: &str) -> Vec<String> {
        let tree = match self.parser.parse(source, None) {
            Some(t) => t,
            None => return vec![],
        };
        let mut imports = Vec::new();
        let root = tree.root_node();
        self.collect_imports(root, source, &mut imports);
        imports
    }

    fn collect_imports(&self, node: tree_sitter::Node, source: &str, imports: &mut Vec<String>) {
        if node.kind() == "use_declaration" {
            imports.push(source[node.start_byte()..node.end_byte()].trim().to_string());
        }
        for child in node.children(&mut node.walk()) {
            self.collect_imports(child, source, imports);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_function() {
        let mut parser = AstParser::new().unwrap();
        let src = "fn hello() -> String { \"world\".into() }";
        let syms = parser.parse_symbols(src, "test.rs");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "hello");
        assert_eq!(syms[0].kind, "function");
    }

    #[test]
    fn test_parse_struct_and_fn() {
        let mut parser = AstParser::new().unwrap();
        let src = "struct Config { port: u16 } fn main() {}";
        let syms = parser.parse_symbols(src, "test.rs");
        assert_eq!(syms.len(), 2);
        assert_eq!(syms[0].kind, "struct");
        assert_eq!(syms[1].kind, "function");
    }

    #[test]
    fn test_imports() {
        let mut parser = AstParser::new().unwrap();
        let src = "use std::collections::HashMap;\nuse crate::config::Config;";
        let imports = parser.parse_imports(src);
        assert_eq!(imports.len(), 2);
    }

    #[test]
    fn test_references() {
        let parser = AstParser::new().unwrap();
        let src = "fn main() {\n  hello();\n  hello();\n}";
        let refs = parser.find_references(src, "hello", "test.rs");
        assert_eq!(refs.len(), 2);
    }
}
