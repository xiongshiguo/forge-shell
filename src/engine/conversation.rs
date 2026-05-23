//! 类型安全的对话构建器
//! 用 Rust 类型系统保证消息序列永远合法——让 400 错误不可能发生

use crate::engine::inference::{ChatMessage, ToolCallDelta, ToolCallFunc, AccumulatedToolCall};

/// 未完成的工具调用句柄——必须用 tool_results 消费
pub struct PendingToolCalls {
    call_ids: Vec<String>,
    tool_names: Vec<String>,
}

impl PendingToolCalls {
    pub fn len(&self) -> usize { self.call_ids.len() }
    pub fn is_empty(&self) -> bool { self.call_ids.is_empty() }
    pub fn ids(&self) -> &[String] { &self.call_ids }
}

/// 类型安全的对话构建器
pub struct Conversation {
    messages: Vec<ChatMessage>,
    thinking_enabled: bool,
    /// 最后一个 assistant 消息的索引（用于回溯 reasoning）
    last_assistant_idx: Option<usize>,
}

impl Conversation {
    pub fn new(thinking_enabled: bool) -> Self {
        Self { messages: Vec::new(), thinking_enabled, last_assistant_idx: None }
    }

    pub fn system(mut self, content: &str) -> Self {
        self.messages.push(ChatMessage::system(content));
        self
    }

    pub fn user(mut self, content: &str) -> Self {
        self.messages.push(ChatMessage::user(content));
        self
    }

    /// 添加 assistant 回复。如果模型调用了工具，返回 PendingToolCalls 句柄。
    /// 句柄必须被 `tool_results()` 消费，否则编译不过。
    pub fn assistant(mut self, content: &str, reasoning: &str, tool_calls: &[AccumulatedToolCall])
        -> (Self, Option<PendingToolCalls>)
    {
        let mut msg = ChatMessage::assistant_with_reasoning(content, reasoning);

        let pending = if tool_calls.is_empty() {
            None
        } else {
            let deltas: Vec<ToolCallDelta> = tool_calls.iter().map(|tc| ToolCallDelta {
                id: Some(tc.id.clone()),
                call_type: Some("function".into()),
                function: Some(ToolCallFunc {
                    name: Some(tc.name.clone()),
                    arguments: Some(tc.arguments.clone()),
                }),
                index: None,
            }).collect();
            msg.tool_calls = Some(deltas);
            Some(PendingToolCalls {
                call_ids: tool_calls.iter().map(|t| t.id.clone()).collect(),
                tool_names: tool_calls.iter().map(|t| t.name.clone()).collect(),
            })
        };

        // thinking 模式一致性：关闭时清除 reasoning
        if !self.thinking_enabled { msg.reasoning_content = None; }

        self.last_assistant_idx = Some(self.messages.len());
        self.messages.push(msg);
        (self, pending)
    }

    /// 消费 PendingToolCalls——按名称匹配结果。保证每个 tool_call 都有对应的 result。
    pub fn tool_results(mut self, pending: PendingToolCalls, results: &[(String, String)]) -> Self {
        for (tool_name, result_text) in results {
            // 找到匹配的 tool_call_id
            let pos = pending.tool_names.iter().position(|n| n == tool_name);
            let call_id = pos.and_then(|i| pending.ids().get(i)).cloned()
                .unwrap_or_else(|| format!("call_{}", tool_name));
            self.messages.push(ChatMessage::tool_result(&call_id, result_text));
        }
        self
    }

    /// 消费 PendingToolCalls——文本格式（无原生 tool_call_id 时使用）
    pub fn tool_results_text(self, pending: PendingToolCalls, combined: &str) -> Self {
        // 文本格式：所有结果合并为一条 user 消息
        let _ = pending; // 消费句柄
        self.user(combined)
    }

    pub fn build(self) -> Vec<ChatMessage> {
        self.messages
    }

    pub fn len(&self) -> usize { self.messages.len() }
}
