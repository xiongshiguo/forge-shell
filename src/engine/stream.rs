//! SSE 流处理器（L3）
//! 保证：chunk 要么解析成功要么记录跳过、错误不重复、内容不丢失

use crate::engine::inference::{StreamChunk, AccumulatedToolCall};
use crate::error::ForgeError;

/// 流式响应累积器
pub struct StreamAccumulator {
    pub content: String,
    pub reasoning: String,
    pub tool_calls: Vec<AccumulatedToolCall>,
    pub finish_reason: Option<String>,
    pub error_count: u32,
    pub has_content: bool,
    max_errors: u32,
}

impl StreamAccumulator {
    pub fn new() -> Self {
        Self {
            content: String::new(),
            reasoning: String::new(),
            tool_calls: Vec::new(),
            finish_reason: None,
            error_count: 0,
            has_content: false,
            max_errors: 3,
        }
    }

    /// 处理一个 SSE chunk。返回 Some(error) 仅在首次错误时。
    pub fn ingest(&mut self, result: Result<StreamChunk, ForgeError>) -> Result<(), ForgeError> {
        match result {
            Ok(chunk) => {
                self.error_count = 0; // 重置
                if !chunk.content.is_empty() {
                    self.has_content = true;
                    self.content.push_str(&chunk.content);
                }
                if !chunk.reasoning_content.is_empty() {
                    self.reasoning.push_str(&chunk.reasoning_content);
                }
                if !chunk.tool_calls.is_empty() {
                    self.tool_calls = chunk.tool_calls;
                }
                if chunk.finish_reason.is_some() {
                    self.finish_reason = chunk.finish_reason;
                }
                Ok(())
            }
            Err(e) => {
                self.error_count += 1;
                if self.error_count <= 1 {
                    Err(e) // 只向上传播一次
                } else if self.error_count >= self.max_errors {
                    Err(ForgeError::Api("流已中断（连续错误过多）".into()))
                } else {
                    Ok(()) // 吞掉中间错误
                }
            }
        }
    }

    /// 是否有有效内容（可降级使用）
    pub fn can_degrade(&self) -> bool {
        self.has_content || !self.tool_calls.is_empty()
    }

    /// 是否已完成
    pub fn is_done(&self) -> bool {
        self.finish_reason.is_some()
    }
}
