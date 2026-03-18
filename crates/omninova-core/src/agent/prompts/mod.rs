//! Personality-Based Prompt Templates for AI Agents
//!
//! This module provides system prompt generation for agents based on their MBTI personality type.
//! It leverages the existing PersonalityConfig from the soul module and adds communication
//! style enhancements.

mod mbti_prompts;

pub use mbti_prompts::{get_system_prompt_for_mbti, get_enhanced_system_prompt};