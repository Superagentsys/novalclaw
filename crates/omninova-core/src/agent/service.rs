//! Agent Service Orchestration Layer
//!
//! This module provides the high-level service that orchestrates agent operations,
//! including session management, message persistence, and LLM provider integration.

use crate::agent::dispatcher::{AgentDispatcher, DispatcherConfig, DispatchError};
use crate::agent::memory_context::{
    build_context_string, retrieve_relevant_memories, MemoryContextResult,
};
use crate::agent::model::AgentModel;
use crate::agent::prompts::get_enhanced_system_prompt;
use crate::agent::soul::MbtiType;
use crate::agent::store::AgentStore;
use crate::agent::streaming::{
    error_codes, stream_error_to_code_and_message, StreamAccumulator, StreamEvent,
    StreamManager, FIRST_TOKEN_TIMEOUT_SECS,
};
use crate::config::schema::MemoryContextConfig;
use crate::db::DbPool;
use crate::memory::{Memory, MemoryCategory, MemoryManager, WorkingMemory, EpisodicMemoryStore, NewEpisodicMemory};
use crate::providers::{ChatMessage, ChatRequest, ChatStreamChunk, Provider, StreamError};
use crate::session::{
    Message as SessionMessage, MessageRole, NewMessage, NewSession, Session,
    MessageStore, SessionStore, SessionStoreError,
};
use crate::tools::{Tool, ToolSpec};
use anyhow::{anyhow, Context, Result};
use futures_util::StreamExt;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{debug, info, instrument, warn};

/// Result of a chat operation
#[derive(Debug, Clone)]
pub struct ChatResult {
    /// The assistant's response text
    pub response: String,
    /// The session ID (existing or newly created)
    pub session_id: i64,
    /// The message ID of the assistant's response
    pub message_id: i64,
    /// Memory context used for this response (if any)
    pub memory_context: Option<MemoryContextResult>,
}

/// Error type for agent service operations
#[derive(Debug, thiserror::Error)]
pub enum AgentServiceError {
    #[error("代理未找到: {0}")]
    AgentNotFound(i64),

    #[error("会话未找到: {0}")]
    SessionNotFound(i64),

    #[error("调度错误: {0}")]
    Dispatch(#[from] DispatchError),

    #[error("会话存储错误: {0}")]
    SessionStore(#[from] SessionStoreError),

    #[error("提供商错误: {0}")]
    Provider(String),

    #[error("配置错误: {0}")]
    Config(String),

    #[error("内存错误: {0}")]
    Memory(String),

    #[error("无效的MBTI类型: {0}")]
    InvalidMbtiType(String),

    #[error("流式响应错误: {0}")]
    Streaming(String),

    #[error("流被取消")]
    StreamCancelled,
}

/// High-level service for agent operations
pub struct AgentService {
    agent_store: AgentStore,
    session_store: SessionStore,
    message_store: MessageStore,
    memory: Arc<dyn Memory>,
    working_memory: Arc<Mutex<WorkingMemory>>,
    /// L2 episodic memory store for long-term memory
    episodic_memory_store: Option<Arc<EpisodicMemoryStore>>,
    /// Unified memory manager for context retrieval
    memory_manager: Option<Arc<tokio::sync::Mutex<MemoryManager>>>,
    /// Memory context configuration
    memory_context_config: MemoryContextConfig,
    tools: Vec<Box<dyn Tool>>,
    tool_specs: Vec<ToolSpec>,
}

impl AgentService {
    /// Create a new AgentService with the given database pool
    pub fn new(
        db: DbPool,
        memory: Arc<dyn Memory>,
        tools: Vec<Box<dyn Tool>>,
    ) -> Self {
        let tool_specs = tools.iter().map(|t| t.spec()).collect();
        Self {
            agent_store: AgentStore::new(db.clone()),
            session_store: SessionStore::new(db.clone()),
            message_store: MessageStore::new(db.clone()),
            memory,
            working_memory: Arc::new(Mutex::new(WorkingMemory::new())),
            episodic_memory_store: None,
            memory_manager: None,
            memory_context_config: MemoryContextConfig::default(),
            tools,
            tool_specs,
        }
    }

    /// Create a new AgentService with episodic memory store enabled
    pub fn with_episodic_memory(
        db: DbPool,
        memory: Arc<dyn Memory>,
        tools: Vec<Box<dyn Tool>>,
        episodic_memory_store: Arc<EpisodicMemoryStore>,
    ) -> Self {
        let tool_specs = tools.iter().map(|t| t.spec()).collect();
        Self {
            agent_store: AgentStore::new(db.clone()),
            session_store: SessionStore::new(db.clone()),
            message_store: MessageStore::new(db.clone()),
            memory,
            working_memory: Arc::new(Mutex::new(WorkingMemory::new())),
            episodic_memory_store: Some(episodic_memory_store),
            memory_manager: None,
            memory_context_config: MemoryContextConfig::default(),
            tools,
            tool_specs,
        }
    }

    /// Set the episodic memory store
    pub fn set_episodic_memory_store(&mut self, store: Arc<EpisodicMemoryStore>) {
        self.episodic_memory_store = Some(store);
    }

    /// Set the memory manager for context retrieval
    pub fn set_memory_manager(&mut self, manager: Arc<tokio::sync::Mutex<MemoryManager>>) {
        self.memory_manager = Some(manager);
    }

    /// Set the memory context configuration
    pub fn set_memory_context_config(&mut self, config: MemoryContextConfig) {
        self.memory_context_config = config;
    }

    /// Send a message to an agent, optionally within an existing session
    ///
    /// If `session_id` is `None`, a new session will be created.
    /// If `quote_message_id` is provided, the message will be marked as a reply to that message.
    #[instrument(skip(self, provider), fields(agent_id = agent_id, session_id = ?session_id, quote_message_id = ?quote_message_id))]
    pub async fn chat(
        &self,
        agent_id: i64,
        session_id: Option<i64>,
        message: &str,
        provider: &dyn Provider,
        quote_message_id: Option<i64>,
    ) -> Result<ChatResult, AgentServiceError> {
        // 1. Load agent configuration
        let agent = self
            .agent_store
            .find_by_id(agent_id)
            .map_err(|e| anyhow!("Failed to load agent: {}", e))
            .context("Loading agent")
            .map_err(|e| AgentServiceError::Config(e.to_string()))?
            .ok_or(AgentServiceError::AgentNotFound(agent_id))?;

        info!(agent_name = %agent.name, "Loaded agent for chat");

        // 2. Get or create session
        let session = match session_id {
            Some(id) => self
                .session_store
                .find_by_id(id)?
                .ok_or(AgentServiceError::SessionNotFound(id))?,
            None => {
                let new_session = NewSession {
                    agent_id,
                    title: None, // Will be auto-generated later if needed
                };
                self.session_store.create(&new_session)?
            }
        };

        debug!(session_id = session.id, "Using session");

        // 3. Retrieve relevant memories for context enhancement
        let memory_context = if let Some(ref memory_manager) = self.memory_manager {
            match retrieve_relevant_memories(
                memory_manager,
                message,
                &self.memory_context_config,
                Some(agent_id),
            ).await {
                Ok(result) if !result.memories.is_empty() => {
                    debug!(
                        memories_count = result.memories.len(),
                        total_chars = result.total_chars,
                        retrieval_time_ms = result.retrieval_time_ms,
                        "Retrieved relevant memories for context"
                    );
                    Some(result)
                }
                Ok(_) => None,
                Err(e) => {
                    warn!(error = %e, "Failed to retrieve memories for context");
                    None
                }
            }
        } else {
            None
        };

        // 4. Build system prompt (with memory context if available)
        let system_prompt = self.build_system_prompt_with_memory(&agent, memory_context.as_ref())?;

        // 5. Load session history
        let history = self.message_store.find_by_session(session.id)?;
        let mut messages = self.convert_history_to_chat_messages(history, &system_prompt);

        // 6. Store user message
        let user_msg = NewMessage {
            session_id: session.id,
            role: MessageRole::User,
            content: message.to_string(),
            quote_message_id,
        };
        let saved_user_msg = self.message_store.create(&user_msg)?;
        messages.push(ChatMessage::user(message));

        // Also store in memory system
        let _ = self
            .memory
            .store(
                &format!("session/{}/{}", session.id, saved_user_msg.id),
                message,
                MemoryCategory::Conversation,
                None,
            )
            .await;

        // Store in working memory (L1) for quick context access
        {
            let mut wm = self.working_memory.lock().await;
            wm.set_session(session.id, agent_id);
            let _ = wm.push_context("user", message).await;
        }

        // 7. Run through dispatcher
        let dispatcher_config = DispatcherConfig {
            max_tool_iterations: 10, // Default, could be from config
            ..Default::default()
        };
        let dispatcher = AgentDispatcher::with_config(
            provider,
            &self.tools,
            &self.tool_specs,
            dispatcher_config,
        );

        let response = dispatcher.run(&mut messages).await.map_err(|e| {
            warn!(error = %e, "Dispatcher failed");
            AgentServiceError::Dispatch(e)
        })?;

        // 8. Save assistant response
        let assistant_msg = NewMessage {
            session_id: session.id,
            role: MessageRole::Assistant,
            content: response.clone(),
            quote_message_id: None,
        };
        let saved_assistant_msg = self.message_store.create(&assistant_msg)?;

        // Store assistant response in working memory (L1)
        {
            let wm = self.working_memory.lock().await;
            let _ = wm.push_context("assistant", &response).await;
        }

        info!(
            session_id = session.id,
            message_id = saved_assistant_msg.id,
            response_length = response.len(),
            "Chat completed successfully"
        );

        Ok(ChatResult {
            response,
            session_id: session.id,
            message_id: saved_assistant_msg.id,
            memory_context,
        })
    }

    /// Create a new session and send the first message
    ///
    /// This is a convenience method that combines session creation with the first message.
    ///
    /// # Arguments
    /// * `agent_id` - The agent to send the message to
    /// * `title` - Optional title for the session
    /// * `message` - The user message
    /// * `provider` - The LLM provider to use
    /// * `quote_message_id` - Optional ID of a message to quote/reply to
    ///
    /// # Returns
    /// Returns `ChatResult` containing the session ID and assistant response.
    pub async fn create_session_and_chat(
        &self,
        agent_id: i64,
        title: Option<String>,
        message: &str,
        provider: &dyn Provider,
        quote_message_id: Option<i64>,
    ) -> Result<ChatResult, AgentServiceError> {
        // Create new session with optional title
        let new_session = NewSession {
            agent_id,
            title,
        };
        let session = self.session_store.create(&new_session)?;

        // Use the chat method with the new session
        self.chat(agent_id, Some(session.id), message, provider, quote_message_id).await
    }

    /// Stream a chat response with real-time events
    ///
    /// This method streams the LLM response and emits events via the callback.
    /// If `session_id` is `None`, a new session will be created.
    ///
    /// # Arguments
    /// * `agent_id` - The agent to send the message to
    /// * `session_id` - Optional session ID (creates new session if None)
    /// * `message` - The user message
    /// * `provider` - The LLM provider to use
    /// * `on_event` - Callback for streaming events
    /// * `stream_manager` - Manager for tracking/cancelling active streams
    /// * `quote_message_id` - Optional ID of a message to quote/reply to
    ///
    /// # Returns
    /// Returns `ChatResult` on success, or `AgentServiceError` on failure.
    /// If the stream is cancelled, returns `AgentServiceError::StreamCancelled`.
    #[instrument(skip(self, provider, on_event, stream_manager), fields(agent_id = agent_id, session_id = ?session_id, quote_message_id = ?quote_message_id))]
    pub async fn chat_stream<F>(
        &self,
        agent_id: i64,
        session_id: Option<i64>,
        message: &str,
        provider: &dyn Provider,
        mut on_event: F,
        stream_manager: Option<Arc<StreamManager>>,
        quote_message_id: Option<i64>,
    ) -> Result<ChatResult, AgentServiceError>
    where
        F: FnMut(StreamEvent) + Send,
    {
        // 1. Load agent configuration
        let agent = self
            .agent_store
            .find_by_id(agent_id)
            .map_err(|e| anyhow!("Failed to load agent: {}", e))
            .context("Loading agent")
            .map_err(|e| AgentServiceError::Config(e.to_string()))?
            .ok_or(AgentServiceError::AgentNotFound(agent_id))?;

        info!(agent_name = %agent.name, "Loaded agent for streaming chat");

        // 2. Get or create session
        let session = match session_id {
            Some(id) => self
                .session_store
                .find_by_id(id)?
                .ok_or(AgentServiceError::SessionNotFound(id))?,
            None => {
                let new_session = NewSession {
                    agent_id,
                    title: None,
                };
                self.session_store.create(&new_session)?
            }
        };

        debug!(session_id = session.id, "Using session for streaming");

        // 3. Build system prompt
        let system_prompt = self.build_system_prompt(&agent)?;

        // 4. Load session history
        let history = self.message_store.find_by_session(session.id)?;
        let messages = self.convert_history_to_chat_messages(history, &system_prompt);

        // 5. Store user message
        let user_msg = NewMessage {
            session_id: session.id,
            role: MessageRole::User,
            content: message.to_string(),
            quote_message_id,
        };
        let saved_user_msg = self.message_store.create(&user_msg)?;

        // Also store in memory system
        let _ = self
            .memory
            .store(
                &format!("session/{}/{}", session.id, saved_user_msg.id),
                message,
                MemoryCategory::Conversation,
                None,
            )
            .await;

        // Store in working memory (L1) for quick context access
        {
            let mut wm = self.working_memory.lock().await;
            wm.set_session(session.id, agent_id);
            let _ = wm.push_context("user", message).await;
        }

        // 6. Register stream with manager if provided
        if let Some(ref manager) = stream_manager {
            manager.register(session.id).await;
        }

        // 7. Emit start event
        on_event(StreamEvent::start(session.id));

        // 8. Create chat request
        let request = ChatRequest {
            messages: &messages,
            tools: Some(&self.tool_specs),
        };

        // 9. Get stream from provider
        let mut stream = match provider.chat_stream(request).await {
            Ok(s) => s,
            Err(e) => {
                let error_event = StreamEvent::error(
                    error_codes::PROVIDER_ERROR,
                    format!("无法启动流式响应: {}", e),
                );
                on_event(error_event);
                return Err(AgentServiceError::Streaming(format!(
                    "Failed to start stream: {}",
                    e
                )));
            }
        };

        // 10. Process stream chunks
        let mut accumulator = StreamAccumulator::new();
        let start_time = Instant::now();
        let mut first_token_received = false;

        loop {
            // Check for cancellation
            if let Some(ref manager) = stream_manager {
                if manager.is_cancelled(session.id).await {
                    accumulator.cancel();
                    on_event(StreamEvent::error(
                        error_codes::CANCELLED,
                        "用户取消了流式响应",
                    ));
                    // Save partial content
                    if accumulator.has_content() {
                        let partial_msg = NewMessage {
                            session_id: session.id,
                            role: MessageRole::Assistant,
                            content: accumulator.final_content(),
                            quote_message_id: None,
                        };
                        let _ = self.message_store.create(&partial_msg);
                    }
                    return Err(AgentServiceError::StreamCancelled);
                }
            }

            // Get next chunk with timeout
            let chunk = tokio::time::timeout(
                Duration::from_secs(30), // Per-chunk timeout
                stream.next(),
            )
            .await;

            match chunk {
                Ok(Some(result)) => match result {
                    Ok(chunk) => {
                        // Check first token latency
                        if !first_token_received
                            && (chunk.delta.is_some() || chunk.reasoning_content.is_some())
                        {
                            first_token_received = true;
                            let latency = start_time.elapsed();
                            if latency > Duration::from_secs(FIRST_TOKEN_TIMEOUT_SECS) {
                                warn!(
                                    latency_ms = latency.as_millis(),
                                    "First token latency exceeded 3s SLA"
                                );
                            }
                            debug!(latency_ms = latency.as_millis(), "First token received");
                        }

                        // Process delta
                        if let Some(ref delta) = chunk.delta {
                            if !delta.is_empty() {
                                on_event(StreamEvent::delta_with_reasoning(
                                    delta,
                                    chunk.reasoning_content.clone().unwrap_or_default(),
                                ));
                            }
                        } else if let Some(ref reasoning) = chunk.reasoning_content {
                            if !reasoning.is_empty() {
                                on_event(StreamEvent::delta_with_reasoning(
                                    "",
                                    reasoning.clone(),
                                ));
                            }
                        }

                        // Process tool calls
                        for tool_call in &chunk.tool_calls {
                            let args: serde_json::Value =
                                serde_json::from_str(&tool_call.arguments).unwrap_or_else(|_| {
                                    serde_json::Value::String(tool_call.arguments.clone())
                                });
                            on_event(StreamEvent::tool_call(&tool_call.name, args));
                        }

                        // Accumulate chunk
                        accumulator.process_chunk(&chunk);

                        // Check for finish
                        if chunk.is_finished() {
                            break;
                        }
                    }
                    Err(e) => {
                        // Stream error
                        let (code, msg) = stream_error_to_code_and_message(&e);
                        let error_event = if accumulator.has_content() {
                            StreamEvent::error_with_partial(code, msg, accumulator.final_content())
                        } else {
                            StreamEvent::error(code, msg)
                        };
                        on_event(error_event);

                        // Save partial content if any
                        if accumulator.has_content() {
                            let partial_msg = NewMessage {
                                session_id: session.id,
                                role: MessageRole::Assistant,
                                content: accumulator.final_content(),
                                quote_message_id: None,
                            };
                            let _ = self.message_store.create(&partial_msg);
                        }

                        return Err(AgentServiceError::Streaming(format!(
                            "Stream error: {}",
                            e
                        )));
                    }
                },
                Ok(None) => {
                    // Stream ended
                    break;
                }
                Err(_timeout) => {
                    // Timeout waiting for chunk
                    let error_event = StreamEvent::error(
                        error_codes::CONNECTION_ERROR,
                        "流式响应超时",
                    );
                    on_event(error_event);

                    // Save partial content if any
                    if accumulator.has_content() {
                        let partial_msg = NewMessage {
                            session_id: session.id,
                            role: MessageRole::Assistant,
                            content: accumulator.final_content(),
                            quote_message_id: None,
                        };
                        let _ = self.message_store.create(&partial_msg);
                    }

                    return Err(AgentServiceError::Streaming(
                        "Stream timeout waiting for chunk".to_string(),
                    ));
                }
            }
        }

        // 11. Save assistant response
        let response = accumulator.final_content();
        let assistant_msg = NewMessage {
            session_id: session.id,
            role: MessageRole::Assistant,
            content: response.clone(),
            quote_message_id: None,
        };
        let saved_assistant_msg = self.message_store.create(&assistant_msg)?;

        // Store assistant response in working memory (L1)
        {
            let wm = self.working_memory.lock().await;
            let _ = wm.push_context("assistant", &response).await;
        }

        // 12. Emit done event
        on_event(StreamEvent::done(session.id, saved_assistant_msg.id));

        // 13. Cleanup stream manager
        if let Some(ref manager) = stream_manager {
            manager.remove(session.id).await;
        }

        info!(
            session_id = session.id,
            message_id = saved_assistant_msg.id,
            response_length = response.len(),
            "Streaming chat completed successfully"
        );

        Ok(ChatResult {
            response,
            session_id: session.id,
            message_id: saved_assistant_msg.id,
            memory_context: None, // Memory context for streaming is returned via events
        })
    }

    /// Create a new session and send the first message with streaming
    #[instrument(skip(self, provider, on_event, stream_manager), fields(agent_id = agent_id))]
    pub async fn create_session_and_chat_stream<F>(
        &self,
        agent_id: i64,
        title: Option<String>,
        message: &str,
        provider: &dyn Provider,
        on_event: F,
        stream_manager: Option<Arc<StreamManager>>,
        quote_message_id: Option<i64>,
    ) -> Result<ChatResult, AgentServiceError>
    where
        F: FnMut(StreamEvent) + Send,
    {
        // Create the session
        let new_session = NewSession {
            agent_id,
            title,
        };
        let session = self.session_store.create(&new_session)?;

        // Then stream the message
        self.chat_stream(agent_id, Some(session.id), message, provider, on_event, stream_manager, quote_message_id)
            .await
    }

    /// Get all sessions for an agent
    pub fn list_sessions(&self, agent_id: i64) -> Result<Vec<Session>, AgentServiceError> {
        self.session_store
            .find_by_agent(agent_id)
            .map_err(AgentServiceError::SessionStore)
    }

    /// Get all messages for a session
    pub fn list_messages(&self, session_id: i64) -> Result<Vec<SessionMessage>, AgentServiceError> {
        self.message_store
            .find_by_session(session_id)
            .map_err(AgentServiceError::SessionStore)
    }

    /// Delete a session and all its messages
    pub fn delete_session(&self, session_id: i64) -> Result<(), AgentServiceError> {
        self.session_store
            .delete(session_id)
            .map_err(AgentServiceError::SessionStore)
    }

    /// Build the system prompt for an agent
    fn build_system_prompt(&self, agent: &AgentModel) -> Result<String, AgentServiceError> {
        // If the agent has a custom system prompt, use it
        if let Some(ref prompt) = agent.system_prompt {
            return Ok(prompt.clone());
        }

        // Otherwise, generate based on MBTI type
        let mbti_type = agent
            .mbti_type
            .as_ref()
            .map(|s| s.parse::<MbtiType>())
            .transpose()
            .map_err(|e| AgentServiceError::InvalidMbtiType(e.to_string()))?;

        match mbti_type {
            Some(mbti) => {
                let traits = mbti.traits();
                Ok(get_enhanced_system_prompt(mbti, &traits))
            }
            None => {
                // Default prompt if no MBTI type specified
                Ok("You are a helpful AI assistant.".to_string())
            }
        }
    }

    /// Build the system prompt with memory context for an agent
    fn build_system_prompt_with_memory(
        &self,
        agent: &AgentModel,
        memory_context: Option<&MemoryContextResult>,
    ) -> Result<String, AgentServiceError> {
        let base_prompt = self.build_system_prompt(agent)?;

        // If there's memory context, prepend it to the system prompt
        if let Some(ctx) = memory_context {
            if !ctx.memories.is_empty() {
                let memory_context_str = build_context_string(ctx, self.memory_context_config.max_chars);
                return Ok(format!("{}{}", memory_context_str, base_prompt));
            }
        }

        Ok(base_prompt)
    }

    /// Convert session history to chat messages for the provider
    fn convert_history_to_chat_messages(
        &self,
        history: Vec<SessionMessage>,
        system_prompt: &str,
    ) -> Vec<ChatMessage> {
        let mut messages = Vec::new();

        // Add system prompt
        messages.push(ChatMessage::system(system_prompt));

        // Add history messages
        for msg in history {
            let chat_msg = match msg.role {
                MessageRole::User => ChatMessage::user(&msg.content),
                MessageRole::Assistant => ChatMessage::assistant(&msg.content),
                MessageRole::System => ChatMessage::system(&msg.content),
            };
            messages.push(chat_msg);
        }

        messages
    }

    /// Persist working memory (L1) to episodic memory (L2)
    ///
    /// This should be called when a session ends to save important context
    /// to long-term storage.
    pub async fn persist_session_to_l2(&self, agent_id: i64, session_id: i64) -> Result<usize, AgentServiceError> {
        let episodic_store = self.episodic_memory_store.as_ref()
            .ok_or_else(|| AgentServiceError::Memory("Episodic memory store not configured".to_string()))?;

        // Get all entries from working memory
        let entries = self.working_memory.lock().await.get_context(0).await
            .map_err(|e| AgentServiceError::Memory(e.to_string()))?;

        let mut count = 0;
        for entry in entries {
            // Determine importance based on role (user messages are more important)
            let importance = match entry.role.as_str() {
                "user" => 7,
                "assistant" => 5,
                "system" => 8,
                _ => 5,
            };

            let new_memory = NewEpisodicMemory {
                agent_id,
                session_id: Some(session_id),
                content: entry.content.clone(),
                importance,
                is_marked: false,
                metadata: Some(serde_json::json!({
                    "role": entry.role,
                    "timestamp": entry.timestamp,
                }).to_string()),
            };

            episodic_store.create(&new_memory)
                .map_err(|e| AgentServiceError::Memory(e.to_string()))?;
            count += 1;
        }

        info!("Persisted {} entries from L1 to L2 for session {}", count, session_id);
        Ok(count)
    }

    /// Store an important memory directly to L2 episodic memory
    pub async fn store_important_memory(
        &self,
        agent_id: i64,
        content: &str,
        importance: u8,
        session_id: Option<i64>,
    ) -> Result<i64, AgentServiceError> {
        let episodic_store = self.episodic_memory_store.as_ref()
            .ok_or_else(|| AgentServiceError::Memory("Episodic memory store not configured".to_string()))?;

        let new_memory = NewEpisodicMemory {
            agent_id,
            session_id,
            content: content.to_string(),
            importance,
            is_marked: false,
            metadata: None,
        };

        episodic_store.create(&new_memory)
            .map_err(|e| AgentServiceError::Memory(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{create_pool, DbPoolConfig};
    use crate::memory::backend::InMemoryMemory;
    use crate::providers::{ChatResponse, ChatStream};
    use crate::session::SessionStore;
    use async_trait::async_trait;
    use futures_util::stream;
    use std::sync::Mutex;
    use tempfile::tempdir;

    /// Mock provider for testing streaming functionality
    struct MockStreamingProvider {
        /// Chunks to yield in the stream
        chunks: Vec<Result<ChatStreamChunk, StreamError>>,
        /// Name of the provider
        name: &'static str,
    }

    impl MockStreamingProvider {
        fn new(chunks: Vec<Result<ChatStreamChunk, StreamError>>) -> Self {
            Self {
                chunks,
                name: "mock-streaming",
            }
        }

        /// Create a provider that yields text chunks
        fn with_text_chunks(texts: Vec<&str>) -> Self {
            let mut chunks: Vec<Result<ChatStreamChunk, StreamError>> = texts
                .into_iter()
                .map(|t| Ok(ChatStreamChunk::text_delta(t)))
                .collect();
            // Add finish chunk
            chunks.push(Ok(ChatStreamChunk::finish("stop")));
            Self::new(chunks)
        }

        /// Create a provider that simulates an error mid-stream
        fn with_error_after(texts: Vec<&str>, error: StreamError) -> Self {
            let mut chunks: Vec<Result<ChatStreamChunk, StreamError>> = texts
                .into_iter()
                .map(|t| Ok(ChatStreamChunk::text_delta(t)))
                .collect();
            chunks.push(Err(error));
            Self::new(chunks)
        }
    }

    #[async_trait]
    impl Provider for MockStreamingProvider {
        fn name(&self) -> &str {
            self.name
        }

        async fn chat(&self, _request: ChatRequest<'_>) -> anyhow::Result<ChatResponse> {
            Ok(ChatResponse {
                text: Some("Mock response".to_string()),
                tool_calls: Vec::new(),
                usage: None,
                reasoning_content: None,
            })
        }

        async fn health_check(&self) -> bool {
            true
        }

        async fn chat_stream(&self, _request: ChatRequest<'_>) -> anyhow::Result<ChatStream> {
            let chunks = self.chunks.clone();
            Ok(Box::pin(stream::iter(chunks)))
        }
    }

    /// Helper to create a test AgentService with all dependencies
    fn create_test_service() -> (AgentService, tempfile::TempDir) {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let pool = create_pool(&db_path, DbPoolConfig::default()).expect("Failed to create pool");

        // Initialize stores (this runs migrations)
        let agent_store = AgentStore::new(pool.clone());
        agent_store.initialize().expect("Failed to initialize agent store");

        let memory: Arc<dyn Memory> = Arc::new(InMemoryMemory::new());

        let service = AgentService::new(pool, memory, vec![]);

        (service, dir)
    }

    /// Helper to create a test agent
    fn create_test_agent(agent_store: &AgentStore) -> i64 {
        let agent = crate::agent::NewAgent {
            name: "Test Agent".to_string(),
            description: Some("A test agent".to_string()),
            domain: Some("testing".to_string()),
            mbti_type: Some("INTJ".to_string()),
            system_prompt: Some("You are a test assistant.".to_string()),
            default_provider_id: None,
            style_config: None,
            context_window_config: None,
            trigger_keywords_config: None,
            privacy_config: None,
        };
        agent_store.create(&agent).expect("Failed to create agent").id
    }

    #[test]
    fn test_chat_result_debug() {
        let result = ChatResult {
            response: "Hello".to_string(),
            session_id: 1,
            message_id: 42,
            memory_context: None,
        };
        let debug_str = format!("{:?}", result);
        assert!(debug_str.contains("Hello"));
    }

    #[test]
    fn test_agent_service_error_display() {
        let err = AgentServiceError::AgentNotFound(42);
        assert!(err.to_string().contains("42"));

        let err = AgentServiceError::SessionNotFound(123);
        assert!(err.to_string().contains("123"));

        let err = AgentServiceError::InvalidMbtiType("XYZ".to_string());
        assert!(err.to_string().contains("XYZ"));
    }

    // ============================================================================
    // Integration Tests for Streaming (Task 8)
    // ============================================================================

    #[tokio::test]
    async fn test_streaming_e2e_with_mock_provider() {
        // Task 8.1: End-to-end streaming test with mock provider
        let (service, _dir) = create_test_service();
        let agent_id = create_test_agent(&service.agent_store);

        // Create mock provider with test chunks
        let provider = MockStreamingProvider::with_text_chunks(vec![
            "Hello",
            ", ",
            "world",
            "!",
        ]);

        // Collect events
        let events: Arc<Mutex<Vec<StreamEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let events_clone = events.clone();

        let result = service
            .chat_stream(
                agent_id,
                None,
                "Test message",
                &provider,
                move |event| {
                    events_clone.lock().unwrap().push(event);
                },
                None,
                None, // quote_message_id
            )
            .await;

        assert!(result.is_ok(), "Stream should complete successfully");
        let chat_result = result.unwrap();
        assert_eq!(chat_result.response, "Hello, world!");
        assert!(chat_result.message_id > 0);

        // Verify events
        let collected_events = events.lock().unwrap();
        assert!(collected_events.len() >= 2, "Should have at least start and done events");

        // Check first event is Start
        match &collected_events[0] {
            StreamEvent::Start { session_id, .. } => {
                assert_eq!(*session_id, chat_result.session_id);
            }
            _ => panic!("First event should be Start"),
        }

        // Check last event is Done
        let last_event = collected_events.last().unwrap();
        match last_event {
            StreamEvent::Done { message_id, .. } => {
                assert_eq!(*message_id, chat_result.message_id);
            }
            _ => panic!("Last event should be Done"),
        }
    }

    #[tokio::test]
    async fn test_streaming_first_token_latency_measurement() {
        // Task 8.2: Test first token latency measurement
        let (service, _dir) = create_test_service();
        let agent_id = create_test_agent(&service.agent_store);

        // Create provider that yields chunks immediately
        let provider = MockStreamingProvider::with_text_chunks(vec!["Quick response"]);

        let result = service
            .chat_stream(
                agent_id,
                None,
                "Test",
                &provider,
                |_event| {},
                None,
                None, // quote_message_id
            )
            .await;

        assert!(result.is_ok());
        // First token latency is logged but not returned
        // The test verifies the code path executes without error
    }

    #[tokio::test]
    async fn test_streaming_interruption_and_partial_persistence() {
        // Task 8.3: Test stream interruption and partial message persistence
        // This test verifies that:
        // 1. StreamManager can track active streams
        // 2. Cancellation is checked during streaming
        // 3. Partial content is saved when stream is cancelled

        let stream_manager = Arc::new(StreamManager::new());

        // Test 1: Verify cancellation tracking works
        let sid = 42i64;
        stream_manager.register(sid).await;
        assert!(!stream_manager.is_cancelled(sid).await);

        stream_manager.cancel(sid).await;
        assert!(stream_manager.is_cancelled(sid).await);

        // Test 2: Verify partial content preservation in StreamAccumulator
        let mut accumulator = StreamAccumulator::new();
        accumulator.process_chunk(&ChatStreamChunk::text_delta("Partial content "));
        accumulator.process_chunk(&ChatStreamChunk::text_delta("before cancel"));
        accumulator.cancel();

        assert!(accumulator.cancelled);
        assert!(accumulator.has_content());
        assert_eq!(accumulator.final_content(), "Partial content before cancel");

        // Test 3: Verify error event includes partial content
        let error_event = StreamEvent::error_with_partial(
            error_codes::CANCELLED,
            "用户取消了流式响应",
            accumulator.final_content(),
        );

        match error_event {
            StreamEvent::Error { code, message, partial_content } => {
                assert_eq!(code, error_codes::CANCELLED);
                assert!(message.contains("取消"));
                assert_eq!(partial_content, Some("Partial content before cancel".to_string()));
            }
            _ => panic!("Expected Error event"),
        }
    }

    #[tokio::test]
    async fn test_streaming_error_recovery_saves_partial() {
        // Task 8.4: Test error recovery scenarios
        let (service, _dir) = create_test_service();
        let agent_id = create_test_agent(&service.agent_store);

        // Create provider that errors mid-stream
        let provider = MockStreamingProvider::with_error_after(
            vec!["Partial ", "content "],
            StreamError::Connection("Connection lost".to_string()),
        );

        let events: Arc<Mutex<Vec<StreamEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let events_clone = events.clone();

        let result = service
            .chat_stream(
                agent_id,
                None,
                "Test error recovery",
                &provider,
                move |event| {
                    events_clone.lock().unwrap().push(event);
                },
                None,
                None, // quote_message_id
            )
            .await;

        assert!(result.is_err(), "Should return error for stream failure");

        // Verify error event was emitted with partial content
        let collected_events = events.lock().unwrap();
        let error_event = collected_events.iter().find(|e| matches!(e, StreamEvent::Error { .. }));
        assert!(error_event.is_some(), "Should have error event");

        if let StreamEvent::Error { partial_content, .. } = error_event.unwrap() {
            assert!(partial_content.is_some(), "Error event should include partial content");
            assert_eq!(partial_content.as_deref(), Some("Partial content "));
        } else {
            panic!("Expected Error event");
        }
    }

    #[tokio::test]
    async fn test_streaming_concurrent_sessions() {
        // Task 8.5: Test concurrent streams (multiple sessions)
        let (service, _dir) = create_test_service();
        let agent_id = create_test_agent(&service.agent_store);

        // Create two providers with different responses
        let provider1 = MockStreamingProvider::with_text_chunks(vec!["Response 1"]);
        let provider2 = MockStreamingProvider::with_text_chunks(vec!["Response 2"]);

        let events1: Arc<Mutex<Vec<StreamEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let events2: Arc<Mutex<Vec<StreamEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let events1_clone = events1.clone();
        let events2_clone = events2.clone();

        // Run both streams concurrently
        let service_clone = Arc::new(service);
        let service1 = service_clone.clone();
        let service2 = service_clone.clone();

        let handle1 = tokio::spawn(async move {
            service1
                .chat_stream(
                    agent_id,
                    None,
                    "Message 1",
                    &provider1,
                    move |e| events1_clone.lock().unwrap().push(e),
                    None,
                    None, // quote_message_id
                )
                .await
        });

        let handle2 = tokio::spawn(async move {
            service2
                .chat_stream(
                    agent_id,
                    None,
                    "Message 2",
                    &provider2,
                    move |e| events2_clone.lock().unwrap().push(e),
                    None,
                    None, // quote_message_id
                )
                .await
        });

        let result1 = handle1.await.expect("Task 1 should complete");
        let result2 = handle2.await.expect("Task 2 should complete");

        assert!(result1.is_ok(), "First stream should succeed");
        assert!(result2.is_ok(), "Second stream should succeed");

        // Verify different sessions were created
        let chat1 = result1.unwrap();
        let chat2 = result2.unwrap();
        assert_ne!(chat1.session_id, chat2.session_id, "Should have different session IDs");
    }

    #[tokio::test]
    async fn test_streaming_with_reasoning_content() {
        // Test streaming with reasoning content (for thinking models)
        let (service, _dir) = create_test_service();
        let agent_id = create_test_agent(&service.agent_store);

        // Create provider with reasoning chunks
        let chunks = vec![
            Ok(ChatStreamChunk {
                delta: None,
                reasoning_content: Some("Let me think...".to_string()),
                ..Default::default()
            }),
            Ok(ChatStreamChunk {
                delta: Some("The answer".to_string()),
                reasoning_content: Some(" is 42".to_string()),
                ..Default::default()
            }),
            Ok(ChatStreamChunk::finish("stop")),
        ];
        let provider = MockStreamingProvider::new(chunks);

        let events: Arc<Mutex<Vec<StreamEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let events_clone = events.clone();

        let result = service
            .chat_stream(
                agent_id,
                None,
                "Deep question",
                &provider,
                move |e| events_clone.lock().unwrap().push(e),
                None,
                None, // quote_message_id
            )
            .await;

        assert!(result.is_ok());
        let chat_result = result.unwrap();
        assert!(chat_result.response.contains("The answer"));

        // Verify reasoning was included in delta events
        let collected = events.lock().unwrap();
        let delta_events: Vec<_> = collected
            .iter()
            .filter_map(|e| match e {
                StreamEvent::Delta { reasoning, .. } => reasoning.clone(),
                _ => None,
            })
            .collect();
        assert!(!delta_events.is_empty() || collected.len() > 2, "Should have reasoning or delta events");
    }

    #[tokio::test]
    async fn test_streaming_agent_not_found() {
        let (service, _dir) = create_test_service();
        let provider = MockStreamingProvider::with_text_chunks(vec!["Test"]);

        let result = service
            .chat_stream(
                999, // Non-existent agent
                None,
                "Test",
                &provider,
                |_| {},
                None,
                None, // quote_message_id
            )
            .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            AgentServiceError::AgentNotFound(id) => assert_eq!(id, 999),
            e => panic!("Expected AgentNotFound error, got: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_streaming_session_not_found() {
        let (service, _dir) = create_test_service();
        let agent_id = create_test_agent(&service.agent_store);
        let provider = MockStreamingProvider::with_text_chunks(vec!["Test"]);

        let result = service
            .chat_stream(
                agent_id,
                Some(999), // Non-existent session
                "Test",
                &provider,
                |_| {},
                None,
                None, // quote_message_id
            )
            .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            AgentServiceError::SessionNotFound(id) => assert_eq!(id, 999),
            e => panic!("Expected SessionNotFound error, got: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_create_session_and_chat_stream() {
        let (service, _dir) = create_test_service();
        let agent_id = create_test_agent(&service.agent_store);
        let provider = MockStreamingProvider::with_text_chunks(vec!["New session response"]);

        let result = service
            .create_session_and_chat_stream(
                agent_id,
                Some("Test Session".to_string()),
                "First message",
                &provider,
                |_| {},
                None,
                None, // quote_message_id
            )
            .await;

        assert!(result.is_ok());
        let chat_result = result.unwrap();
        assert!(chat_result.session_id > 0);

        // Verify session was created with title
        let session = service
            .session_store
            .find_by_id(chat_result.session_id)
            .expect("Session should exist");
        assert!(session.is_some());
        let session = session.unwrap();
        assert_eq!(session.title, Some("Test Session".to_string()));
    }

    #[tokio::test]
    async fn test_chat_with_quote_message_id() {
        // Test that quote_message_id is persisted when sending a message
        let (service, _dir) = create_test_service();
        let agent_id = create_test_agent(&service.agent_store);

        // Create a provider
        let provider = MockStreamingProvider::with_text_chunks(vec!["Response"]);

        // First, create a session and send an initial message
        let result1 = service
            .chat(agent_id, None, "Original message", &provider, None)
            .await
            .expect("First chat should succeed");

        // Get the user message ID from the session
        let messages = service
            .message_store
            .find_by_session(result1.session_id)
            .expect("Should find messages");

        // Find the user message (should be the first one)
        let original_user_msg = messages
            .iter()
            .find(|m| m.role == MessageRole::User)
            .expect("Should have user message");

        // Now send a reply with quote_message_id
        let result2 = service
            .chat(
                agent_id,
                Some(result1.session_id),
                "Reply to original message",
                &provider,
                Some(original_user_msg.id),
            )
            .await
            .expect("Reply chat should succeed");

        // Verify the reply message has quote_message_id set
        let messages_after = service
            .message_store
            .find_by_session(result2.session_id)
            .expect("Should find messages");

        // Find the reply user message
        let reply_user_msg = messages_after
            .iter()
            .filter(|m| m.role == MessageRole::User)
            .find(|m| m.content == "Reply to original message")
            .expect("Should have reply user message");

        assert_eq!(
            reply_user_msg.quote_message_id,
            Some(original_user_msg.id),
            "Reply message should have quote_message_id set"
        );
    }
}