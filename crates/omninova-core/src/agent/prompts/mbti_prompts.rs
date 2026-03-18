//! MBTI Personality Prompt Templates
//!
//! Provides system prompt generation functions for agents based on their MBTI personality type.
//! Leverages the existing PersonalityConfig from the soul module and adds communication style
//! enhancements.

use crate::agent::soul::{MbtiType, PersonalityTraits};

/// Returns the base system prompt for a given MBTI type.
///
/// This function retrieves the pre-defined system prompt template from the PersonalityConfig
/// associated with the given MBTI type.
///
/// # Arguments
///
/// * `mbti_type` - The MBTI personality type
///
/// # Returns
///
/// A system prompt string tailored to the personality type
pub fn get_system_prompt_for_mbti(mbti_type: MbtiType) -> String {
    mbti_type.config().system_prompt_template
}

/// Returns an enhanced system prompt that includes communication style hints.
///
/// This function combines the base personality prompt with communication style
/// guidance derived from the personality traits, providing a more nuanced system
/// prompt for agent responses.
///
/// # Arguments
///
/// * `mbti_type` - The MBTI personality type
/// * `traits` - The personality traits containing communication style and behavior tendencies
///
/// # Returns
///
/// An enhanced system prompt string with communication style hints
pub fn get_enhanced_system_prompt(mbti_type: MbtiType, traits: &PersonalityTraits) -> String {
    let base_prompt = get_system_prompt_for_mbti(mbti_type);

    // Build communication style hints
    let style_hints = format_communication_style_hints(&traits.communication_style);

    // Build behavior tendency hints
    let behavior_hints = format_behavior_hints(&traits.behavior_tendency);

    // Combine into enhanced prompt
    format!(
        "{}\n\n---\n\n**沟通风格指导:**\n{}\n\n**行为倾向:**\n{}",
        base_prompt, style_hints, behavior_hints
    )
}

/// Formats communication style into readable hints for the AI.
fn format_communication_style_hints(style: &crate::agent::soul::CommunicationStyle) -> String {
    let traits_str = style
        .language_traits
        .iter()
        .map(|t| format!("- {}", t))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "- 偏好: {}\n- 语言特征:\n{}\n- 反馈风格: {}",
        style.preference, traits_str, style.feedback_style
    )
}

/// Formats behavior tendencies into readable hints for the AI.
fn format_behavior_hints(tendency: &crate::agent::soul::BehaviorTendency) -> String {
    format!(
        "- 决策方式: {}\n- 信息处理: {}\n- 社交互动: {}\n- 压力应对: {}",
        tendency.decision_making,
        tendency.information_processing,
        tendency.social_interaction,
        tendency.stress_response
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_system_prompt_for_mbti_intj() {
        let prompt = get_system_prompt_for_mbti(MbtiType::Intj);
        assert!(prompt.contains("INTJ"));
        assert!(prompt.contains("战略思维"));
    }

    #[test]
    fn test_get_system_prompt_for_mbti_enfp() {
        let prompt = get_system_prompt_for_mbti(MbtiType::Enfp);
        assert!(prompt.contains("ENFP"));
        assert!(prompt.contains("热情"));
    }

    #[test]
    fn test_get_enhanced_system_prompt() {
        let traits = MbtiType::Intj.traits();
        let prompt = get_enhanced_system_prompt(MbtiType::Intj, &traits);

        // Should contain base prompt
        assert!(prompt.contains("INTJ"));
        // Should contain style hints section
        assert!(prompt.contains("沟通风格指导"));
        // Should contain behavior hints section
        assert!(prompt.contains("行为倾向"));
        // Should contain behavior fields
        assert!(prompt.contains("决策方式"));
        assert!(prompt.contains("信息处理"));
    }

    #[test]
    fn test_all_mbti_types_have_prompts() {
        for mbti in MbtiType::all() {
            let prompt = get_system_prompt_for_mbti(mbti);
            assert!(!prompt.is_empty(), "Prompt for {:?} should not be empty", mbti);
            // Each prompt should contain the MBTI type name
            let type_str = format!("{:?}", mbti).to_uppercase();
            assert!(
                prompt.contains(&type_str),
                "Prompt for {:?} should contain the type name",
                mbti
            );
        }
    }

    #[test]
    fn test_format_communication_style_hints() {
        let style = MbtiType::Intj.communication_style();
        let hints = format_communication_style_hints(&style);

        assert!(hints.contains("偏好"));
        assert!(hints.contains("语言特征"));
        assert!(hints.contains("反馈风格"));
    }

    #[test]
    fn test_format_behavior_hints() {
        let tendency = MbtiType::Intj.behavior_tendency();
        let hints = format_behavior_hints(&tendency);

        assert!(hints.contains("决策方式"));
        assert!(hints.contains("信息处理"));
        assert!(hints.contains("社交互动"));
        assert!(hints.contains("压力应对"));
    }
}