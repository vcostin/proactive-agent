use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A single labeled block in the assembled context, with a pre-computed token estimate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextBlock {
    pub label: String,
    pub content: String,
    pub token_count: usize,
}

impl ContextBlock {
    pub fn new(label: impl Into<String>, content: impl Into<String>) -> Self {
        let content = content.into();
        // EXTEND: replace with actual tokenizer (tiktoken-rs or llama.cpp /tokenize endpoint)
        let token_count = estimate_tokens(&content);
        Self { label: label.into(), content, token_count }
    }
}

/// A single conversation turn stored in the recent-turns window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Turn {
    pub role: String,
    pub content: String,
}

/// OpenAI-compatible message used when serialising the context for the adapter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

/// Fully assembled context for one LLM call.
/// The persona block is always first and is never derived from memory retrieval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssembledContext {
    pub persona: ContextBlock,
    pub semantic: ContextBlock,
    pub episodic: ContextBlock,
    pub recent_turns: Vec<Turn>,
    pub user_input: String,
    pub assembled_at: DateTime<Utc>,
}

impl AssembledContext {
    /// Construct an assembled context from pre-retrieved memory blocks.
    pub fn assemble(
        persona: &str,
        semantic_facts: &[String],
        episodic_memories: &[String],
        recent_turns: Vec<Turn>,
        user_input: String,
    ) -> Self {
        let semantic_content = if semantic_facts.is_empty() {
            String::new()
        } else {
            semantic_facts.join("\n")
        };

        let episodic_content = if episodic_memories.is_empty() {
            String::new()
        } else {
            episodic_memories.join("\n---\n")
        };

        Self {
            persona: ContextBlock::new("persona", persona),
            semantic: ContextBlock::new("semantic", semantic_content),
            episodic: ContextBlock::new("episodic", episodic_content),
            recent_turns,
            user_input,
            assembled_at: Utc::now(),
        }
    }

    /// Convert to the OpenAI chat messages format sent to the adapter.
    /// Layout: system (persona + memory blocks) → recent turns → current user input.
    pub fn to_messages(&self) -> Vec<Message> {
        let mut system_parts = vec![self.persona.content.clone()];
        if !self.semantic.content.is_empty() {
            system_parts.push(format!(
                "## What I know about you:\n{}",
                self.semantic.content
            ));
        }
        if !self.episodic.content.is_empty() {
            system_parts.push(format!(
                "## Relevant past context:\n{}",
                self.episodic.content
            ));
        }

        let mut messages = vec![Message {
            role: "system".to_string(),
            content: system_parts.join("\n\n"),
        }];

        for turn in &self.recent_turns {
            messages.push(Message { role: turn.role.clone(), content: turn.content.clone() });
        }

        messages.push(Message { role: "user".to_string(), content: self.user_input.clone() });
        messages
    }

    pub fn total_tokens(&self) -> usize {
        self.persona.token_count
            + self.semantic.token_count
            + self.episodic.token_count
            + self.recent_turns.iter().map(|t| estimate_tokens(&t.content)).sum::<usize>()
            + estimate_tokens(&self.user_input)
    }
}

/// Rough token estimate: ~1.3 tokens per whitespace-delimited word.
fn estimate_tokens(text: &str) -> usize {
    (text.split_whitespace().count() * 4).div_ceil(3)
}
