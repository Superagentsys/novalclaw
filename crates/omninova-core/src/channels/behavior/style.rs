//! Response Style Types and Processor
//!
//! Defines different response styles and provides processing utilities.

use serde::{Deserialize, Serialize};

/// Response style for channel messages
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ResponseStyle {
    /// Formal, professional tone
    Formal,
    /// Casual, friendly tone
    Casual,
    /// Brief, concise responses
    Concise,
    /// Detailed, comprehensive responses
    #[default]
    Detailed,
}

impl std::fmt::Display for ResponseStyle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Formal => write!(f, "formal"),
            Self::Casual => write!(f, "casual"),
            Self::Concise => write!(f, "concise"),
            Self::Detailed => write!(f, "detailed"),
        }
    }
}

impl std::str::FromStr for ResponseStyle {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "formal" => Ok(Self::Formal),
            "casual" => Ok(Self::Casual),
            "concise" => Ok(Self::Concise),
            "detailed" => Ok(Self::Detailed),
            _ => Err(format!("Unknown response style: {}", s)),
        }
    }
}

/// Processor for applying response styles to messages
pub struct ResponseStyleProcessor;

impl ResponseStyleProcessor {
    /// Apply a response style to the content
    ///
    /// This method transforms the content based on the specified style:
    /// - Formal: Professional language, complete sentences
    /// - Casual: Friendly language, may include emojis
    /// - Concise: Brief responses, bullet points
    /// - Detailed: Comprehensive explanations with context
    pub fn apply_style(content: &str, style: ResponseStyle) -> String {
        match style {
            ResponseStyle::Formal => Self::apply_formal(content),
            ResponseStyle::Casual => Self::apply_casual(content),
            ResponseStyle::Concise => Self::apply_concise(content),
            ResponseStyle::Detailed => content.to_string(),
        }
    }

    /// Apply formal style transformations
    fn apply_formal(content: &str) -> String {
        // Remove excessive emojis
        let mut result = Self::remove_emojis(content);

        // Ensure proper capitalization at sentence starts
        let mut chars: Vec<char> = result.chars().collect();
        let mut capitalize_next = true;

        for ch in chars.iter_mut() {
            if capitalize_next && ch.is_alphabetic() {
                *ch = ch.to_uppercase().next().unwrap_or(*ch);
                capitalize_next = false;
            }
            if *ch == '.' || *ch == '!' || *ch == '?' {
                capitalize_next = true;
            }
        }

        result = chars.into_iter().collect();
        result
    }

    /// Apply casual style transformations
    fn apply_casual(content: &str) -> String {
        // Casual style keeps the content mostly as-is
        // In a real implementation, this could add friendly greetings
        // or adjust tone. For now, we just ensure friendly presentation.
        content.to_string()
    }

    /// Apply concise style transformations
    fn apply_concise(content: &str) -> String {
        // Split into sentences and take first few
        let sentences: Vec<&str> = content
            .split(|c| c == '.' || c == '!' || c == '?')
            .filter(|s| !s.trim().is_empty())
            .collect();

        if sentences.is_empty() {
            return content.to_string();
        }

        // Take at most 2-3 sentences for concise response
        let max_sentences = 3;
        let concise: Vec<&str> = sentences.into_iter().take(max_sentences).collect();

        // Join with proper punctuation
        let mut result = concise.join(". ");
        if !result.ends_with('.') && !result.ends_with('!') && !result.ends_with('?') {
            result.push('.');
        }

        result
    }

    /// Remove emojis from text
    fn remove_emojis(text: &str) -> String {
        text.chars()
            .filter(|c| !is_emoji(*c))
            .collect()
    }

    /// Truncate content to a maximum length
    ///
    /// Tries to break at sentence boundaries when possible.
    pub fn truncate(content: &str, max_length: usize) -> String {
        if content.len() <= max_length {
            return content.to_string();
        }

        // Find a good break point
        let break_chars = ['.', '!', '?', '\n', ' '];
        let mut break_point = max_length;

        // Look for sentence boundary before max_length
        for (i, ch) in content.char_indices() {
            if i >= max_length {
                break;
            }
            if break_chars.contains(&ch) {
                break_point = i + 1;
            }
        }

        // If no good break point found, just cut at max_length
        if break_point > max_length {
            break_point = max_length;
        }

        let mut truncated = content[..break_point].to_string();
        if !truncated.ends_with('.') && !truncated.ends_with('!') && !truncated.ends_with('?') {
            truncated.push_str("...");
        }

        truncated
    }
}

/// Check if a character is an emoji
fn is_emoji(c: char) -> bool {
    // Common emoji Unicode ranges
    let cp = c as u32;
    (0x1F600..=0x1F64F).contains(&cp)  // Emoticons
        || (0x1F300..=0x1F5FF).contains(&cp)  // Misc Symbols and Pictographs
        || (0x1F680..=0x1F6FF).contains(&cp)  // Transport and Map
        || (0x1F700..=0x1F77F).contains(&cp)  // Alchemical Symbols
        || (0x1F780..=0x1F7FF).contains(&cp)  // Geometric Shapes Extended
        || (0x1F800..=0x1F8FF).contains(&cp)  // Supplemental Arrows-C
        || (0x1F900..=0x1F9FF).contains(&cp)  // Supplemental Symbols and Pictographs
        || (0x1FA00..=0x1FA6F).contains(&cp)  // Chess Symbols
        || (0x1FA70..=0x1FAFF).contains(&cp)  // Symbols and Pictographs Extended-A
        || (0x2600..=0x26FF).contains(&cp)    // Misc symbols
        || (0x2700..=0x27BF).contains(&cp)    // Dingbats
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_style_display() {
        assert_eq!(ResponseStyle::Formal.to_string(), "formal");
        assert_eq!(ResponseStyle::Casual.to_string(), "casual");
        assert_eq!(ResponseStyle::Concise.to_string(), "concise");
        assert_eq!(ResponseStyle::Detailed.to_string(), "detailed");
    }

    #[test]
    fn test_style_from_str() {
        assert_eq!(ResponseStyle::from_str("formal").unwrap(), ResponseStyle::Formal);
        assert_eq!(ResponseStyle::from_str("CASUAL").unwrap(), ResponseStyle::Casual);
        assert!(ResponseStyle::from_str("unknown").is_err());
    }

    #[test]
    fn test_apply_detailed_style() {
        let content = "This is a test message.";
        let result = ResponseStyleProcessor::apply_style(content, ResponseStyle::Detailed);
        assert_eq!(result, content);
    }

    #[test]
    fn test_apply_concise_style() {
        let content = "First sentence. Second sentence. Third sentence. Fourth sentence.";
        let result = ResponseStyleProcessor::apply_style(content, ResponseStyle::Concise);
        // Should limit to 3 sentences
        assert!(result.matches('.').count() <= 3);
    }

    #[test]
    fn test_truncate() {
        let content = "This is a long sentence that needs to be truncated at some point.";
        let result = ResponseStyleProcessor::truncate(content, 30);
        assert!(result.len() <= 35); // Allow for "..." suffix
        assert!(result.ends_with('.') || result.ends_with("..."));
    }

    #[test]
    fn test_truncate_short_content() {
        let content = "Short.";
        let result = ResponseStyleProcessor::truncate(content, 100);
        assert_eq!(result, content);
    }

    #[test]
    fn test_serialization() {
        let style = ResponseStyle::Casual;
        let json = serde_json::to_string(&style).unwrap();
        assert_eq!(json, "\"casual\"");

        let deserialized: ResponseStyle = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, style);
    }
}