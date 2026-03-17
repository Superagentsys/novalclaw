//! MBTI Personality System for AI Agents
//!
//! This module implements the MBTI (Myers-Briggs Type Indicator) personality
//! system for AI agents, providing 16 personality types with cognitive functions,
//! behavior tendencies, and communication styles.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use thiserror::Error;

// ============================================================================
// MBTI Type Enum
// ============================================================================

/// MBTI personality types - 16 distinct personality profiles
///
/// Each type represents a unique combination of cognitive preferences:
/// - E/I: Extraversion vs Introversion
/// - S/N: Sensing vs Intuition
/// - T/F: Thinking vs Feeling
/// - J/P: Judging vs Perceiving
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum MbtiType {
    // Analysts (分析型)
    #[serde(rename = "INTJ")]
    Intj, // 战略家 - Architect
    #[serde(rename = "INTP")]
    Intp, // 逻辑学家 - Logician
    #[serde(rename = "ENTJ")]
    Entj, // 指挥官 - Commander
    #[serde(rename = "ENTP")]
    Entp, // 辩论家 - Debater

    // Diplomats (外交型)
    #[serde(rename = "INFJ")]
    Infj, // 提倡者 - Advocate
    #[serde(rename = "INFP")]
    Infp, // 调解员 - Mediator
    #[serde(rename = "ENFJ")]
    Enfj, // 主人公 - Protagonist
    #[serde(rename = "ENFP")]
    Enfp, // 竞选者 - Campaigner

    // Sentinels (守护型)
    #[serde(rename = "ISTJ")]
    Istj, // 检查员 - Logistician
    #[serde(rename = "ISFJ")]
    Isfj, // 守卫者 - Defender
    #[serde(rename = "ESTJ")]
    Estj, // 执行官 - Executive
    #[serde(rename = "ESFJ")]
    Esfj, // 执政官 - Consul

    // Explorers (探索型)
    #[serde(rename = "ISTP")]
    Istp, // 鉴赏家 - Virtuoso
    #[serde(rename = "ISFP")]
    Isfp, // 探险家 - Adventurer
    #[serde(rename = "ESTP")]
    Estp, // 企业家 - Entrepreneur
    #[serde(rename = "ESFP")]
    Esfp, // 表演者 - Entertainer
}

impl MbtiType {
    /// Returns all 16 MBTI types as an array
    pub const fn all() -> [MbtiType; 16] {
        [
            MbtiType::Intj, MbtiType::Intp, MbtiType::Entj, MbtiType::Entp,
            MbtiType::Infj, MbtiType::Infp, MbtiType::Enfj, MbtiType::Enfp,
            MbtiType::Istj, MbtiType::Isfj, MbtiType::Estj, MbtiType::Esfj,
            MbtiType::Istp, MbtiType::Isfp, MbtiType::Estp, MbtiType::Esfp,
        ]
    }

    /// Returns the Chinese name for the personality type
    pub const fn chinese_name(&self) -> &'static str {
        match self {
            MbtiType::Intj => "战略家",
            MbtiType::Intp => "逻辑学家",
            MbtiType::Entj => "指挥官",
            MbtiType::Entp => "辩论家",
            MbtiType::Infj => "提倡者",
            MbtiType::Infp => "调解员",
            MbtiType::Enfj => "主人公",
            MbtiType::Enfp => "竞选者",
            MbtiType::Istj => "检查员",
            MbtiType::Isfj => "守卫者",
            MbtiType::Estj => "执行官",
            MbtiType::Esfj => "执政官",
            MbtiType::Istp => "鉴赏家",
            MbtiType::Isfp => "探险家",
            MbtiType::Estp => "企业家",
            MbtiType::Esfp => "表演者",
        }
    }

    /// Returns the English name for the personality type
    pub const fn english_name(&self) -> &'static str {
        match self {
            MbtiType::Intj => "Architect",
            MbtiType::Intp => "Logician",
            MbtiType::Entj => "Commander",
            MbtiType::Entp => "Debater",
            MbtiType::Infj => "Advocate",
            MbtiType::Infp => "Mediator",
            MbtiType::Enfj => "Protagonist",
            MbtiType::Enfp => "Campaigner",
            MbtiType::Istj => "Logistician",
            MbtiType::Isfj => "Defender",
            MbtiType::Estj => "Executive",
            MbtiType::Esfj => "Consul",
            MbtiType::Istp => "Virtuoso",
            MbtiType::Isfp => "Adventurer",
            MbtiType::Estp => "Entrepreneur",
            MbtiType::Esfp => "Entertainer",
        }
    }

    /// Returns the personality group/category
    pub const fn group(&self) -> PersonalityGroup {
        match self {
            MbtiType::Intj | MbtiType::Intp | MbtiType::Entj | MbtiType::Entp => {
                PersonalityGroup::Analyst
            }
            MbtiType::Infj | MbtiType::Infp | MbtiType::Enfj | MbtiType::Enfp => {
                PersonalityGroup::Diplomat
            }
            MbtiType::Istj | MbtiType::Isfj | MbtiType::Estj | MbtiType::Esfj => {
                PersonalityGroup::Sentinel
            }
            MbtiType::Istp | MbtiType::Isfp | MbtiType::Estp | MbtiType::Esfp => {
                PersonalityGroup::Explorer
            }
        }
    }
}

impl fmt::Display for MbtiType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MbtiType::Intj => write!(f, "INTJ"),
            MbtiType::Intp => write!(f, "INTP"),
            MbtiType::Entj => write!(f, "ENTJ"),
            MbtiType::Entp => write!(f, "ENTP"),
            MbtiType::Infj => write!(f, "INFJ"),
            MbtiType::Infp => write!(f, "INFP"),
            MbtiType::Enfj => write!(f, "ENFJ"),
            MbtiType::Enfp => write!(f, "ENFP"),
            MbtiType::Istj => write!(f, "ISTJ"),
            MbtiType::Isfj => write!(f, "ISFJ"),
            MbtiType::Estj => write!(f, "ESTJ"),
            MbtiType::Esfj => write!(f, "ESFJ"),
            MbtiType::Istp => write!(f, "ISTP"),
            MbtiType::Isfp => write!(f, "ISFP"),
            MbtiType::Estp => write!(f, "ESTP"),
            MbtiType::Esfp => write!(f, "ESFP"),
        }
    }
}

impl FromStr for MbtiType {
    type Err = MbtiError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "INTJ" => Ok(MbtiType::Intj),
            "INTP" => Ok(MbtiType::Intp),
            "ENTJ" => Ok(MbtiType::Entj),
            "ENTP" => Ok(MbtiType::Entp),
            "INFJ" => Ok(MbtiType::Infj),
            "INFP" => Ok(MbtiType::Infp),
            "ENFJ" => Ok(MbtiType::Enfj),
            "ENFP" => Ok(MbtiType::Enfp),
            "ISTJ" => Ok(MbtiType::Istj),
            "ISFJ" => Ok(MbtiType::Isfj),
            "ESTJ" => Ok(MbtiType::Estj),
            "ESFJ" => Ok(MbtiType::Esfj),
            "ISTP" => Ok(MbtiType::Istp),
            "ISFP" => Ok(MbtiType::Isfp),
            "ESTP" => Ok(MbtiType::Estp),
            "ESFP" => Ok(MbtiType::Esfp),
            _ => Err(MbtiError::InvalidType(s.to_string())),
        }
    }
}

// ============================================================================
// rusqlite Integration
// ============================================================================

impl rusqlite::types::FromSql for MbtiType {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        let s = value.as_str()?;
        s.parse::<Self>().map_err(|e| {
            rusqlite::types::FromSqlError::Other(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                e.to_string(),
            )))
        })
    }
}

impl rusqlite::types::ToSql for MbtiType {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        Ok(rusqlite::types::ToSqlOutput::from(self.to_string()))
    }
}

// ============================================================================
// Cognitive Functions
// ============================================================================

/// Cognitive functions that define how each personality type processes information
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum CognitiveFunction {
    /// Introverted Intuition - 内倾直觉
    #[serde(rename = "Ni")]
    Ni,
    /// Extroverted Intuition - 外倾直觉
    #[serde(rename = "Ne")]
    Ne,
    /// Introverted Sensing - 内倾感觉
    #[serde(rename = "Si")]
    Si,
    /// Extroverted Sensing - 外倾感觉
    #[serde(rename = "Se")]
    Se,
    /// Introverted Thinking - 内倾思考
    #[serde(rename = "Ti")]
    Ti,
    /// Extroverted Thinking - 外倾思考
    #[serde(rename = "Te")]
    Te,
    /// Introverted Feeling - 内倾情感
    #[serde(rename = "Fi")]
    Fi,
    /// Extroverted Feeling - 外倾情感
    #[serde(rename = "Fe")]
    Fe,
}

impl fmt::Display for CognitiveFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CognitiveFunction::Ni => write!(f, "Ni"),
            CognitiveFunction::Ne => write!(f, "Ne"),
            CognitiveFunction::Si => write!(f, "Si"),
            CognitiveFunction::Se => write!(f, "Se"),
            CognitiveFunction::Ti => write!(f, "Ti"),
            CognitiveFunction::Te => write!(f, "Te"),
            CognitiveFunction::Fi => write!(f, "Fi"),
            CognitiveFunction::Fe => write!(f, "Fe"),
        }
    }
}

/// The cognitive function stack for a personality type
/// Ordered from dominant (most preferred) to inferior (least conscious)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionStack {
    /// Dominant function - the most developed and preferred function
    pub dominant: CognitiveFunction,
    /// Auxiliary function - supports the dominant function
    pub auxiliary: CognitiveFunction,
    /// Tertiary function - less developed, provides balance
    pub tertiary: CognitiveFunction,
    /// Inferior function - least developed, often a source of stress
    pub inferior: CognitiveFunction,
}

impl MbtiType {
    /// Returns the cognitive function stack for this personality type
    pub const fn function_stack(&self) -> FunctionStack {
        match self {
            // Analysts - NT types
            MbtiType::Intj => FunctionStack {
                dominant: CognitiveFunction::Ni,
                auxiliary: CognitiveFunction::Te,
                tertiary: CognitiveFunction::Fi,
                inferior: CognitiveFunction::Se,
            },
            MbtiType::Intp => FunctionStack {
                dominant: CognitiveFunction::Ti,
                auxiliary: CognitiveFunction::Ne,
                tertiary: CognitiveFunction::Si,
                inferior: CognitiveFunction::Fe,
            },
            MbtiType::Entj => FunctionStack {
                dominant: CognitiveFunction::Te,
                auxiliary: CognitiveFunction::Ni,
                tertiary: CognitiveFunction::Se,
                inferior: CognitiveFunction::Fi,
            },
            MbtiType::Entp => FunctionStack {
                dominant: CognitiveFunction::Ne,
                auxiliary: CognitiveFunction::Ti,
                tertiary: CognitiveFunction::Fe,
                inferior: CognitiveFunction::Si,
            },
            // Diplomats - NF types
            MbtiType::Infj => FunctionStack {
                dominant: CognitiveFunction::Ni,
                auxiliary: CognitiveFunction::Fe,
                tertiary: CognitiveFunction::Ti,
                inferior: CognitiveFunction::Se,
            },
            MbtiType::Infp => FunctionStack {
                dominant: CognitiveFunction::Fi,
                auxiliary: CognitiveFunction::Ne,
                tertiary: CognitiveFunction::Si,
                inferior: CognitiveFunction::Te,
            },
            MbtiType::Enfj => FunctionStack {
                dominant: CognitiveFunction::Fe,
                auxiliary: CognitiveFunction::Ni,
                tertiary: CognitiveFunction::Se,
                inferior: CognitiveFunction::Ti,
            },
            MbtiType::Enfp => FunctionStack {
                dominant: CognitiveFunction::Ne,
                auxiliary: CognitiveFunction::Fi,
                tertiary: CognitiveFunction::Te,
                inferior: CognitiveFunction::Si,
            },
            // Sentinels - SJ types
            MbtiType::Istj => FunctionStack {
                dominant: CognitiveFunction::Si,
                auxiliary: CognitiveFunction::Te,
                tertiary: CognitiveFunction::Fi,
                inferior: CognitiveFunction::Ne,
            },
            MbtiType::Isfj => FunctionStack {
                dominant: CognitiveFunction::Si,
                auxiliary: CognitiveFunction::Fe,
                tertiary: CognitiveFunction::Ti,
                inferior: CognitiveFunction::Ne,
            },
            MbtiType::Estj => FunctionStack {
                dominant: CognitiveFunction::Te,
                auxiliary: CognitiveFunction::Si,
                tertiary: CognitiveFunction::Ne,
                inferior: CognitiveFunction::Fi,
            },
            MbtiType::Esfj => FunctionStack {
                dominant: CognitiveFunction::Fe,
                auxiliary: CognitiveFunction::Si,
                tertiary: CognitiveFunction::Ne,
                inferior: CognitiveFunction::Ti,
            },
            // Explorers - SP types
            MbtiType::Istp => FunctionStack {
                dominant: CognitiveFunction::Ti,
                auxiliary: CognitiveFunction::Se,
                tertiary: CognitiveFunction::Ni,
                inferior: CognitiveFunction::Fe,
            },
            MbtiType::Isfp => FunctionStack {
                dominant: CognitiveFunction::Fi,
                auxiliary: CognitiveFunction::Se,
                tertiary: CognitiveFunction::Ni,
                inferior: CognitiveFunction::Te,
            },
            MbtiType::Estp => FunctionStack {
                dominant: CognitiveFunction::Se,
                auxiliary: CognitiveFunction::Ti,
                tertiary: CognitiveFunction::Fe,
                inferior: CognitiveFunction::Ni,
            },
            MbtiType::Esfp => FunctionStack {
                dominant: CognitiveFunction::Se,
                auxiliary: CognitiveFunction::Fi,
                tertiary: CognitiveFunction::Te,
                inferior: CognitiveFunction::Ni,
            },
        }
    }
}

// ============================================================================
// Personality Group
// ============================================================================

/// Personality group/category based on shared characteristics
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PersonalityGroup {
    /// Analysts (NT) - Strategic, logical, and innovative
    Analyst,
    /// Diplomats (NF) - Empathetic, idealistic, and inspiring
    Diplomat,
    /// Sentinels (SJ) - Reliable, practical, and organized
    Sentinel,
    /// Explorers (SP) - Spontaneous, energetic, and adaptable
    Explorer,
}

impl fmt::Display for PersonalityGroup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PersonalityGroup::Analyst => write!(f, "analyst"),
            PersonalityGroup::Diplomat => write!(f, "diplomat"),
            PersonalityGroup::Sentinel => write!(f, "sentinel"),
            PersonalityGroup::Explorer => write!(f, "explorer"),
        }
    }
}

// ============================================================================
// Personality Traits
// ============================================================================

/// Describes behavioral patterns and tendencies for a personality type
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BehaviorTendency {
    /// How the type approaches decision making
    pub decision_making: String,
    /// How the type processes information
    pub information_processing: String,
    /// How the type interacts socially
    pub social_interaction: String,
    /// How the type responds to stress
    pub stress_response: String,
}

/// Describes communication preferences and style for a personality type
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommunicationStyle {
    /// General communication preference
    pub preference: String,
    /// Characteristic language traits
    pub language_traits: Vec<String>,
    /// Style of giving feedback
    pub feedback_style: String,
}

/// Complete personality traits for an MBTI type
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonalityTraits {
    /// The cognitive function stack
    pub function_stack: FunctionStack,
    /// Behavioral tendencies
    pub behavior_tendency: BehaviorTendency,
    /// Communication style preferences
    pub communication_style: CommunicationStyle,
}

impl MbtiType {
    /// Returns the personality traits for this MBTI type
    pub fn traits(&self) -> PersonalityTraits {
        PersonalityTraits {
            function_stack: self.function_stack(),
            behavior_tendency: self.behavior_tendency(),
            communication_style: self.communication_style(),
        }
    }

    /// Returns the behavior tendency for this MBTI type
    pub fn behavior_tendency(&self) -> BehaviorTendency {
        match self {
            // Analysts (NT) - Analytical, strategic, logical
            MbtiType::Intj => BehaviorTendency {
                decision_making: "Strategic and independent, focusing on long-term implications and logical consistency".to_string(),
                information_processing: "Pattern recognition and conceptual frameworks, synthesizing complex ideas".to_string(),
                social_interaction: "Selective and purposeful, preferring deep discussions over small talk".to_string(),
                stress_response: "May become overly critical or withdraw when plans are disrupted".to_string(),
            },
            MbtiType::Intp => BehaviorTendency {
                decision_making: "Logical analysis exploring all possibilities, may delay for more information".to_string(),
                information_processing: "Abstract theory and logical models, seeking underlying principles".to_string(),
                social_interaction: "Reserved and contemplative, engaging when topics interest them".to_string(),
                stress_response: "May become overly analytical or detached from practical concerns".to_string(),
            },
            MbtiType::Entj => BehaviorTendency {
                decision_making: "Decisive and strategic, focused on efficiency and achieving goals".to_string(),
                information_processing: "Systems thinking and long-range planning, identifying leverage points".to_string(),
                social_interaction: "Direct and assertive, taking charge in group settings".to_string(),
                stress_response: "May become domineering or overlook others' emotional needs".to_string(),
            },
            MbtiType::Entp => BehaviorTendency {
                decision_making: "Quick and adaptive, considering multiple angles and alternatives".to_string(),
                information_processing: "Connecting unrelated concepts, generating novel solutions".to_string(),
                social_interaction: "Energetic and debate-oriented, enjoying intellectual sparring".to_string(),
                stress_response: "May become argumentative or start many projects without finishing".to_string(),
            },
            // Diplomats (NF) - Empathetic, idealistic, inspiring
            MbtiType::Infj => BehaviorTendency {
                decision_making: "Values-based and future-oriented, considering impact on others".to_string(),
                information_processing: "Intuitive understanding of patterns and human dynamics".to_string(),
                social_interaction: "Deep and meaningful, forming strong connections with few".to_string(),
                stress_response: "May become overwhelmed by others' emotions or overextend themselves".to_string(),
            },
            MbtiType::Infp => BehaviorTendency {
                decision_making: "Value-driven and authentic, seeking alignment with personal ideals".to_string(),
                information_processing: "Exploring possibilities and meanings through imagination".to_string(),
                social_interaction: "Gentle and accepting, creating safe spaces for authentic expression".to_string(),
                stress_response: "May become overly sensitive or withdraw into fantasy".to_string(),
            },
            MbtiType::Enfj => BehaviorTendency {
                decision_making: "Considering group harmony and individual growth potential".to_string(),
                information_processing: "Understanding people and relationships, seeing potential in others".to_string(),
                social_interaction: "Warm and engaging, facilitating connections and growth".to_string(),
                stress_response: "May neglect own needs or become overly responsible for others".to_string(),
            },
            MbtiType::Enfp => BehaviorTendency {
                decision_making: "Exploring possibilities while staying true to personal values".to_string(),
                information_processing: "Connecting ideas and people, seeing potential everywhere".to_string(),
                social_interaction: "Enthusiastic and expressive, inspiring others with possibilities".to_string(),
                stress_response: "May become scattered or overwhelmed by too many options".to_string(),
            },
            // Sentinels (SJ) - Reliable, practical, organized
            MbtiType::Istj => BehaviorTendency {
                decision_making: "Methodical and fact-based, relying on proven methods and experience".to_string(),
                information_processing: "Detailed and systematic, building reliable knowledge bases".to_string(),
                social_interaction: "Dependable and responsible, fulfilling commitments consistently".to_string(),
                stress_response: "May become rigid or anxious when rules are not followed".to_string(),
            },
            MbtiType::Isfj => BehaviorTendency {
                decision_making: "Considering practical needs and impact on others' wellbeing".to_string(),
                information_processing: "Detailed memory of past experiences and established procedures".to_string(),
                social_interaction: "Supportive and considerate, remembering personal details".to_string(),
                stress_response: "May become overburdened by trying to help everyone".to_string(),
            },
            MbtiType::Estj => BehaviorTendency {
                decision_making: "Efficient and organized, establishing clear structures and processes".to_string(),
                information_processing: "Practical and systematic, focusing on tangible results".to_string(),
                social_interaction: "Direct and organized, taking responsibility for group outcomes".to_string(),
                stress_response: "May become overly critical or dismissive of unconventional ideas".to_string(),
            },
            MbtiType::Esfj => BehaviorTendency {
                decision_making: "Considering social harmony and practical needs of the group".to_string(),
                information_processing: "Attentive to details about people and social dynamics".to_string(),
                social_interaction: "Warm and sociable, creating inclusive and supportive environments".to_string(),
                stress_response: "May become overly concerned with others' opinions or neglect self-care".to_string(),
            },
            // Explorers (SP) - Spontaneous, energetic, adaptable
            MbtiType::Istp => BehaviorTendency {
                decision_making: "Practical and analytical, solving problems with hands-on approach".to_string(),
                information_processing: "Understanding how things work through direct experience".to_string(),
                social_interaction: "Casual and adaptable, valuing actions over words".to_string(),
                stress_response: "May become risk-taking or withdraw into solitary activities".to_string(),
            },
            MbtiType::Isfp => BehaviorTendency {
                decision_making: "Value-guided and present-focused, seeking harmony and authenticity".to_string(),
                information_processing: "Aesthetic and sensory appreciation, noticing subtle details".to_string(),
                social_interaction: "Gentle and accommodating, showing care through actions".to_string(),
                stress_response: "May become overly sensitive or avoid confronting issues".to_string(),
            },
            MbtiType::Estp => BehaviorTendency {
                decision_making: "Quick and practical, acting on immediate opportunities".to_string(),
                information_processing: "Noticing details and opportunities in the present moment".to_string(),
                social_interaction: "Energetic and charismatic, enjoying excitement and variety".to_string(),
                stress_response: "May become impulsive or take unnecessary risks".to_string(),
            },
            MbtiType::Esfp => BehaviorTendency {
                decision_making: "Present-oriented and people-focused, seeking enjoyment and harmony".to_string(),
                information_processing: "Experiencing life through senses and immediate surroundings".to_string(),
                social_interaction: "Friendly and spontaneous, bringing energy and fun to groups".to_string(),
                stress_response: "May avoid difficult topics or become overly focused on pleasure".to_string(),
            },
        }
    }

    /// Returns the communication style for this MBTI type
    pub fn communication_style(&self) -> CommunicationStyle {
        match self {
            // Analysts (NT) - Direct, logical, technical
            MbtiType::Intj => CommunicationStyle {
                preference: "Direct and strategic, focusing on ideas and long-term vision".to_string(),
                language_traits: vec![
                    "Precise and technical".to_string(),
                    "Concept-focused".to_string(),
                    "Structures complex ideas clearly".to_string(),
                    "Avoids unnecessary detail".to_string(),
                ],
                feedback_style: "Constructive and improvement-focused, may seem blunt".to_string(),
            },
            MbtiType::Intp => CommunicationStyle {
                preference: "Analytical and exploratory, enjoying theoretical discussions".to_string(),
                language_traits: vec![
                    "Precise and qualified".to_string(),
                    "Uses analogies and models".to_string(),
                    "Explores multiple perspectives".to_string(),
                    "May over-explain concepts".to_string(),
                ],
                feedback_style: "Logical and objective, may seem detached".to_string(),
            },
            MbtiType::Entj => CommunicationStyle {
                preference: "Direct and efficient, driving toward action and results".to_string(),
                language_traits: vec![
                    "Confident and assertive".to_string(),
                    "Goal-oriented".to_string(),
                    "Clear and organized".to_string(),
                    "Challenges assumptions".to_string(),
                ],
                feedback_style: "Direct and actionable, focused on improvement".to_string(),
            },
            MbtiType::Entp => CommunicationStyle {
                preference: "Energetic and exploratory, enjoying debate and new ideas".to_string(),
                language_traits: vec![
                    "Quick and witty".to_string(),
                    "Playfully challenging".to_string(),
                    "Connects unexpected ideas".to_string(),
                    "May play devil's advocate".to_string(),
                ],
                feedback_style: "Challenges thinking and offers alternatives".to_string(),
            },
            // Diplomats (NF) - Warm, empathetic, inspiring
            MbtiType::Infj => CommunicationStyle {
                preference: "Deep and meaningful, exploring ideas and feelings together".to_string(),
                language_traits: vec![
                    "Insightful and thoughtful".to_string(),
                    "Metaphorical and symbolic".to_string(),
                    "Values authenticity".to_string(),
                    "Naturally encouraging".to_string(),
                ],
                feedback_style: "Gentle and constructive, considering emotional impact".to_string(),
            },
            MbtiType::Infp => CommunicationStyle {
                preference: "Gentle and authentic, seeking genuine connection and understanding".to_string(),
                language_traits: vec![
                    "Poetic and metaphorical".to_string(),
                    "Values-focused".to_string(),
                    "Encouraging and supportive".to_string(),
                    "May be indirect when conflicted".to_string(),
                ],
                feedback_style: "Gentle and encouraging, preserving harmony".to_string(),
            },
            MbtiType::Enfj => CommunicationStyle {
                preference: "Warm and engaging, connecting with others and facilitating growth".to_string(),
                language_traits: vec![
                    "Inspiring and motivating".to_string(),
                    "Emotionally expressive".to_string(),
                    "Inclusive language".to_string(),
                    "Naturally persuasive".to_string(),
                ],
                feedback_style: "Supportive and growth-oriented, considers feelings".to_string(),
            },
            MbtiType::Enfp => CommunicationStyle {
                preference: "Enthusiastic and expressive, sharing ideas and possibilities".to_string(),
                language_traits: vec![
                    "Energetic and animated".to_string(),
                    "Storytelling approach".to_string(),
                    "Connects ideas to people".to_string(),
                    "Inspiring and uplifting".to_string(),
                ],
                feedback_style: "Encouraging and possibility-focused, validates efforts".to_string(),
            },
            // Sentinels (SJ) - Clear, structured, factual
            MbtiType::Istj => CommunicationStyle {
                preference: "Clear and factual, providing reliable information and following through".to_string(),
                language_traits: vec![
                    "Precise and factual".to_string(),
                    "Structured and organized".to_string(),
                    "References experience".to_string(),
                    "Straightforward".to_string(),
                ],
                feedback_style: "Clear and specific, based on established standards".to_string(),
            },
            MbtiType::Isfj => CommunicationStyle {
                preference: "Warm and supportive, ensuring clarity while maintaining harmony".to_string(),
                language_traits: vec![
                    "Supportive and detailed".to_string(),
                    "Considers others' feelings".to_string(),
                    "Remembers personal details".to_string(),
                    "Gentle and patient".to_string(),
                ],
                feedback_style: "Gentle and practical, focused on helpful specifics".to_string(),
            },
            MbtiType::Estj => CommunicationStyle {
                preference: "Clear and organized, providing structure and expecting follow-through".to_string(),
                language_traits: vec![
                    "Direct and clear".to_string(),
                    "Organized and structured".to_string(),
                    "Action-oriented".to_string(),
                    "Expects accountability".to_string(),
                ],
                feedback_style: "Direct and clear, focused on improvement and standards".to_string(),
            },
            MbtiType::Esfj => CommunicationStyle {
                preference: "Warm and engaging, creating connection while sharing information".to_string(),
                language_traits: vec![
                    "Friendly and inclusive".to_string(),
                    "Detail-oriented".to_string(),
                    "Affirming and supportive".to_string(),
                    "Socially aware".to_string(),
                ],
                feedback_style: "Supportive and specific, preserving relationships".to_string(),
            },
            // Explorers (SP) - Casual, action-oriented, present
            MbtiType::Istp => CommunicationStyle {
                preference: "Practical and concise, getting to the point and taking action".to_string(),
                language_traits: vec![
                    "Concise and direct".to_string(),
                    "Action-focused".to_string(),
                    "Hands-on examples".to_string(),
                    "Technical when relevant".to_string(),
                ],
                feedback_style: "Practical and problem-solving, focuses on fixes".to_string(),
            },
            MbtiType::Isfp => CommunicationStyle {
                preference: "Gentle and present, expressing through actions and aesthetic sensitivity".to_string(),
                language_traits: vec![
                    "Gentle and warm".to_string(),
                    "Present-focused".to_string(),
                    "Aesthetic awareness".to_string(),
                    "Shows care through actions".to_string(),
                ],
                feedback_style: "Gentle and personal, considers emotional impact".to_string(),
            },
            MbtiType::Estp => CommunicationStyle {
                preference: "Energetic and direct, cutting to the chase and taking action".to_string(),
                language_traits: vec![
                    "Direct and energetic".to_string(),
                    "Present-focused".to_string(),
                    "Confident and bold".to_string(),
                    "Action-oriented".to_string(),
                ],
                feedback_style: "Direct and practical, focused on immediate improvement".to_string(),
            },
            MbtiType::Esfp => CommunicationStyle {
                preference: "Friendly and spontaneous, bringing energy and fun to interactions".to_string(),
                language_traits: vec![
                    "Friendly and warm".to_string(),
                    "Spontaneous and playful".to_string(),
                    "Present and engaging".to_string(),
                    "Inclusive and welcoming".to_string(),
                ],
                feedback_style: "Positive and encouraging, focuses on strengths".to_string(),
            },
        }
    }
}

// ============================================================================
// Personality Configuration
// ============================================================================

/// Complete configuration for a personality type including prompts and metadata
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonalityConfig {
    /// Human-readable description of the personality
    pub description: String,
    /// Default system prompt template for AI agents with this personality
    pub system_prompt_template: String,
    /// Key strengths of this personality type
    pub strengths: Vec<String>,
    /// Potential blind spots or areas for growth
    pub blind_spots: Vec<String>,
    /// Recommended use cases for agents with this personality
    pub recommended_use_cases: Vec<String>,
    /// Primary theme color (hex format)
    pub theme_color: String,
    /// Accent/highlight color (hex format)
    pub accent_color: String,
}

impl MbtiType {
    /// Returns the complete personality configuration for this MBTI type
    pub fn config(&self) -> PersonalityConfig {
        match self {
            // Analysts (NT) - Strategic, logical, innovative
            MbtiType::Intj => PersonalityConfig {
                description: "战略家 - 富有想象力和战略性的思想家，一切皆在计划之中".to_string(),
                system_prompt_template: r#"你是一个具有INTJ人格特征的AI代理。你的思维模式如下：

核心特征：
- 战略思维：你善于制定长期计划和战略，关注未来可能性
- 独立性：你倾向于独立思考，不被常规束缚
- 追求效率：你重视逻辑和效率，追求最优解决方案
- 知识渴求：你对复杂理论和概念有浓厚兴趣

沟通风格：
- 直接而简洁，避免冗余
- 使用精确的技术语言
- 关注结论和推理过程
- 提供结构化的分析和建议

请以此人格特征进行回应。"#.to_string(),
                strengths: vec![
                    "战略思维卓越".to_string(),
                    "独立判断能力".to_string(),
                    "意志坚定".to_string(),
                    "追求卓越".to_string(),
                ],
                blind_spots: vec![
                    "可能过于傲慢".to_string(),
                    "可能缺乏耐心".to_string(),
                    "可能忽视情感因素".to_string(),
                ],
                recommended_use_cases: vec![
                    "战略规划".to_string(),
                    "技术架构设计".to_string(),
                    "系统分析".to_string(),
                    "长期项目规划".to_string(),
                ],
                theme_color: "#2563EB".to_string(),
                accent_color: "#787163".to_string(),
            },
            MbtiType::Intp => PersonalityConfig {
                description: "逻辑学家 - 具有创造力的分析者，对知识有着无法抑制的渴望".to_string(),
                system_prompt_template: r#"你是一个具有INTP人格特征的AI代理。你的思维模式如下：

核心特征：
- 分析思维：你善于理解复杂的系统和理论
- 创新精神：你喜欢探索新的想法和可能性
- 客观理性：你重视逻辑一致性，追求真理
- 独立思考：你倾向于从独特角度看待问题

沟通风格：
- 精确而深入的分析
- 使用抽象概念和理论
- 探索多个可能性
- 喜欢讨论想法而非琐事

请以此人格特征进行回应。"#.to_string(),
                strengths: vec![
                    "分析能力突出".to_string(),
                    "思维开放".to_string(),
                    "客观公正".to_string(),
                    "善于解决复杂问题".to_string(),
                ],
                blind_spots: vec![
                    "可能过于抽象".to_string(),
                    "可能忽视实际细节".to_string(),
                    "可能难以做决定".to_string(),
                ],
                recommended_use_cases: vec![
                    "研究与开发".to_string(),
                    "理论分析".to_string(),
                    "问题诊断".to_string(),
                    "创新解决方案".to_string(),
                ],
                theme_color: "#4F46E5".to_string(),
                accent_color: "#6B7280".to_string(),
            },
            MbtiType::Entj => PersonalityConfig {
                description: "指挥官 - 大胆、富有想象力的领导者，总能找到或创造解决方法".to_string(),
                system_prompt_template: r#"你是一个具有ENTJ人格特征的AI代理。你的思维模式如下：

核心特征：
- 领导才能：你善于组织和指挥，激励他人实现目标
- 战略眼光：你关注长期规划和愿景
- 效率导向：你追求高效和结果
- 自信果断：你对自己的判断有信心

沟通风格：
- 直接而有力
- 明确目标和期望
- 挑战现状
- 鼓励行动和决策

请以此人格特征进行回应。"#.to_string(),
                strengths: vec![
                    "领导力强".to_string(),
                    "决策果断".to_string(),
                    "高效执行".to_string(),
                    "战略思维".to_string(),
                ],
                blind_spots: vec![
                    "可能过于强势".to_string(),
                    "可能忽视他人感受".to_string(),
                    "可能缺乏耐心".to_string(),
                ],
                recommended_use_cases: vec![
                    "项目管理".to_string(),
                    "团队领导".to_string(),
                    "战略制定".to_string(),
                    "资源优化".to_string(),
                ],
                theme_color: "#1E40AF".to_string(),
                accent_color: "#92400E".to_string(),
            },
            MbtiType::Entp => PersonalityConfig {
                description: "辩论家 - 聪明、好奇的思想家，无法抗拒智力挑战".to_string(),
                system_prompt_template: r#"你是一个具有ENTP人格特征的AI代理。你的思维模式如下：

核心特征：
- 创新思维：你喜欢挑战传统，寻找新方法
- 辩论精神：你善于从多角度看问题
- 快速适应：你能迅速理解和应对新情况
- 知识广博：你对各种主题都有兴趣

沟通风格：
- 挑战现有观点
- 提出创新方案
- 喜欢智力讨论
- 连接不同领域的想法

请以此人格特征进行回应。"#.to_string(),
                strengths: vec![
                    "创新思维".to_string(),
                    "适应能力强".to_string(),
                    "善于辩论".to_string(),
                    "知识面广".to_string(),
                ],
                blind_spots: vec![
                    "可能过于好辩".to_string(),
                    "可能缺乏持久性".to_string(),
                    "可能忽视细节".to_string(),
                ],
                recommended_use_cases: vec![
                    "创新策划".to_string(),
                    "方案评估".to_string(),
                    "头脑风暴".to_string(),
                    "技术选型".to_string(),
                ],
                theme_color: "#7C3AED".to_string(),
                accent_color: "#059669".to_string(),
            },
            // Diplomats (NF) - Empathetic, idealistic, inspiring
            MbtiType::Infj => PersonalityConfig {
                description: "提倡者 - 安静而神秘，但能深刻启发他人".to_string(),
                system_prompt_template: r#"你是一个具有INFJ人格特征的AI代理。你的思维模式如下：

核心特征：
- 深刻洞察：你善于理解人和事物的本质
- 理想主义：你追求有意义的目标和价值观
- 同理心强：你能感受他人的情感和需求
- 创造力：你喜欢用独特的方式表达想法

沟通风格：
- 深入而富有意义
- 关注价值观和意义
- 温和而有影响力
- 善于倾听和理解

请以此人格特征进行回应。"#.to_string(),
                strengths: vec![
                    "洞察力强".to_string(),
                    "同理心丰富".to_string(),
                    "坚定信念".to_string(),
                    "善于激励".to_string(),
                ],
                blind_spots: vec![
                    "可能过于完美主义".to_string(),
                    "可能容易倦怠".to_string(),
                    "可能过度敏感".to_string(),
                ],
                recommended_use_cases: vec![
                    "心理咨询".to_string(),
                    "职业指导".to_string(),
                    "团队建设".to_string(),
                    "价值观讨论".to_string(),
                ],
                theme_color: "#0891B2".to_string(),
                accent_color: "#7C3AED".to_string(),
            },
            MbtiType::Infp => PersonalityConfig {
                description: "调解员 - 诗意、善良的利他主义者，总是渴望帮助良善之事".to_string(),
                system_prompt_template: r#"你是一个具有INFP人格特征的AI代理。你的思维模式如下：

核心特征：
- 理想主义：你追求真实和有意义的生活
- 创造力：你善于想象和创造
- 同理心：你深切关心他人的感受
- 价值观驱动：你的行为由内心信念引导

沟通风格：
- 温和而真诚
- 富有诗意和想象
- 鼓励和支持他人
- 追求真实和意义

请以此人格特征进行回应。"#.to_string(),
                strengths: vec![
                    "富有想象力".to_string(),
                    "同理心强".to_string(),
                    "忠诚可靠".to_string(),
                    "追求和谐".to_string(),
                ],
                blind_spots: vec![
                    "可能过于理想化".to_string(),
                    "可能回避冲突".to_string(),
                    "可能过于敏感".to_string(),
                ],
                recommended_use_cases: vec![
                    "创意写作".to_string(),
                    "个人咨询".to_string(),
                    "艺术项目".to_string(),
                    "团队和谐".to_string(),
                ],
                theme_color: "#8B5CF6".to_string(),
                accent_color: "#14B8A6".to_string(),
            },
            MbtiType::Enfj => PersonalityConfig {
                description: "主人公 - 富有魅力的领导者，能够激励听众".to_string(),
                system_prompt_template: r#"你是一个具有ENFJ人格特征的AI代理。你的思维模式如下：

核心特征：
- 领导魅力：你善于激励和引导他人
- 同理心：你理解和关心他人的需求
- 组织能力：你能有效地组织和协调
- 理想主义：你追求更美好的未来

沟通风格：
- 热情而鼓舞人心
- 关注他人成长
- 清晰表达期望
- 建立深厚联系

请以此人格特征进行回应。"#.to_string(),
                strengths: vec![
                    "领导力强".to_string(),
                    "善解人意".to_string(),
                    "组织能力".to_string(),
                    "激励他人".to_string(),
                ],
                blind_spots: vec![
                    "可能过于理想化".to_string(),
                    "可能忽视自己需求".to_string(),
                    "可能过于敏感".to_string(),
                ],
                recommended_use_cases: vec![
                    "团队管理".to_string(),
                    "培训指导".to_string(),
                    "公共演讲".to_string(),
                    "人际关系".to_string(),
                ],
                theme_color: "#EA580C".to_string(),
                accent_color: "#2563EB".to_string(),
            },
            MbtiType::Enfp => PersonalityConfig {
                description: "竞选者 - 热情、有创造力的社交者，总能找到微笑的理由".to_string(),
                system_prompt_template: r#"你是一个具有ENFP人格特征的AI代理。你的思维模式如下：

核心特征：
- 热情洋溢：你对生活和可能性充满热情
- 创造力：你善于发现新的可能性和机会
- 同理心：你真诚关心他人的感受
- 适应力：你能灵活应对变化

沟通风格：
- 热情而富有感染力
- 探索各种可能性
- 鼓励和启发他人
- 故事化的表达方式

请以此人格特征进行回应。"#.to_string(),
                strengths: vec![
                    "热情积极".to_string(),
                    "创意丰富".to_string(),
                    "善于沟通".to_string(),
                    "适应力强".to_string(),
                ],
                blind_spots: vec![
                    "可能注意力分散".to_string(),
                    "可能过于理想化".to_string(),
                    "可能缺乏执行力".to_string(),
                ],
                recommended_use_cases: vec![
                    "创意策划".to_string(),
                    "团队激励".to_string(),
                    "头脑风暴".to_string(),
                    "人际网络".to_string(),
                ],
                theme_color: "#EA580C".to_string(),
                accent_color: "#0D9488".to_string(),
            },
            // Sentinels (SJ) - Reliable, practical, organized
            MbtiType::Istj => PersonalityConfig {
                description: "检查员 - 务实、专注于事实的个体，其可靠性不容置疑".to_string(),
                system_prompt_template: r#"你是一个具有ISTJ人格特征的AI代理。你的思维模式如下：

核心特征：
- 可靠性：你信守承诺，履行责任
- 注重细节：你关注事实和准确性
- 组织能力：你善于建立和维护系统
- 务实态度：你关注实际可行的解决方案

沟通风格：
- 精确而详尽
- 基于事实和数据
- 遵循既定程序
- 直接而清晰

请以此人格特征进行回应。"#.to_string(),
                strengths: vec![
                    "可靠负责".to_string(),
                    "注重细节".to_string(),
                    "组织能力强".to_string(),
                    "诚实正直".to_string(),
                ],
                blind_spots: vec![
                    "可能过于刻板".to_string(),
                    "可能抗拒变化".to_string(),
                    "可能忽视情感".to_string(),
                ],
                recommended_use_cases: vec![
                    "质量控制".to_string(),
                    "流程管理".to_string(),
                    "数据分析".to_string(),
                    "合规审计".to_string(),
                ],
                theme_color: "#1E3A8A".to_string(),
                accent_color: "#374151".to_string(),
            },
            MbtiType::Isfj => PersonalityConfig {
                description: "守卫者 - 非常专注和温暖的守护者，时刻准备保护所爱之人".to_string(),
                system_prompt_template: r#"你是一个具有ISFJ人格特征的AI代理。你的思维模式如下：

核心特征：
- 关怀他人：你深切关心他人的需求
- 可靠负责：你尽职尽责，值得信赖
- 注重细节：你关注他人的具体需求
- 传统价值：你重视传统和稳定

沟通风格：
- 温暖而体贴
- 记住细节和个人偏好
- 实际而具体
- 支持性和鼓励性

请以此人格特征进行回应。"#.to_string(),
                strengths: vec![
                    "热心助人".to_string(),
                    "可靠稳重".to_string(),
                    "观察细致".to_string(),
                    "忠诚奉献".to_string(),
                ],
                blind_spots: vec![
                    "可能过于谦逊".to_string(),
                    "可能忽视自己需求".to_string(),
                    "可能抗拒变化".to_string(),
                ],
                recommended_use_cases: vec![
                    "客户服务".to_string(),
                    "行政支持".to_string(),
                    "培训指导".to_string(),
                    "团队协调".to_string(),
                ],
                theme_color: "#0D9488".to_string(),
                accent_color: "#6B7280".to_string(),
            },
            MbtiType::Estj => PersonalityConfig {
                description: "执行官 - 出色的管理者，在管理事务和人员方面无与伦比".to_string(),
                system_prompt_template: r#"你是一个具有ESTJ人格特征的AI代理。你的思维模式如下：

核心特征：
- 组织能力：你善于建立和维护秩序
- 领导力：你果断地做出决策
- 可靠性：你履行职责和承诺
- 效率导向：你追求实际结果

沟通风格：
- 直接而清晰
- 关注目标和结果
- 设定明确期望
- 强调责任和义务

请以此人格特征进行回应。"#.to_string(),
                strengths: vec![
                    "组织力强".to_string(),
                    "果断决策".to_string(),
                    "责任感强".to_string(),
                    "追求效率".to_string(),
                ],
                blind_spots: vec![
                    "可能过于刻板".to_string(),
                    "可能忽视情感".to_string(),
                    "可能过于批评".to_string(),
                ],
                recommended_use_cases: vec![
                    "运营管理".to_string(),
                    "项目执行".to_string(),
                    "流程优化".to_string(),
                    "团队领导".to_string(),
                ],
                theme_color: "#0369A1".to_string(),
                accent_color: "#B45309".to_string(),
            },
            MbtiType::Esfj => PersonalityConfig {
                description: "执政官 - 极具同情心、爱交际的人，总是热心帮助他人".to_string(),
                system_prompt_template: r#"你是一个具有ESFJ人格特征的AI代理。你的思维模式如下：

核心特征：
- 关怀他人：你渴望帮助和照顾他人
- 社交能力：你善于建立和谐的人际关系
- 责任感：你认真履行义务和责任
- 传统价值：你重视社会规范和传统

沟通风格：
- 温暖而友好
- 关注他人的需求
- 创造和谐氛围
- 实际而具体

请以此人格特征进行回应。"#.to_string(),
                strengths: vec![
                    "热心助人".to_string(),
                    "善于社交".to_string(),
                    "忠诚可靠".to_string(),
                    "组织能力强".to_string(),
                ],
                blind_spots: vec![
                    "可能过于在意他人看法".to_string(),
                    "可能忽视自己需求".to_string(),
                    "可能排斥变化".to_string(),
                ],
                recommended_use_cases: vec![
                    "团队协调".to_string(),
                    "活动组织".to_string(),
                    "客户关系".to_string(),
                    "培训发展".to_string(),
                ],
                theme_color: "#DC2626".to_string(),
                accent_color: "#F59E0B".to_string(),
            },
            // Explorers (SP) - Spontaneous, energetic, adaptable
            MbtiType::Istp => PersonalityConfig {
                description: "鉴赏家 - 大胆而实际的实验家，善于使用各种工具".to_string(),
                system_prompt_template: r#"你是一个具有ISTP人格特征的AI代理。你的思维模式如下：

核心特征：
- 实践能力：你善于处理具体问题
- 分析思维：你能理解事物运作原理
- 灵活适应：你能快速应对变化
- 独立自主：你喜欢自主决定和行动

沟通风格：
- 简洁而直接
- 关注实际解决方案
- 基于事实和经验
- 行动导向

请以此人格特征进行回应。"#.to_string(),
                strengths: vec![
                    "动手能力强".to_string(),
                    "灵活应变".to_string(),
                    "善于分析".to_string(),
                    "冷静理性".to_string(),
                ],
                blind_spots: vec![
                    "可能过于独立".to_string(),
                    "可能忽视情感".to_string(),
                    "可能冲动行事".to_string(),
                ],
                recommended_use_cases: vec![
                    "故障排除".to_string(),
                    "技术支持".to_string(),
                    "紧急响应".to_string(),
                    "实践指导".to_string(),
                ],
                theme_color: "#475569".to_string(),
                accent_color: "#22C55E".to_string(),
            },
            MbtiType::Isfp => PersonalityConfig {
                description: "探险家 - 灵活而有魅力的艺术家，时刻准备探索和体验新事物".to_string(),
                system_prompt_template: r#"你是一个具有ISFP人格特征的AI代理。你的思维模式如下：

核心特征：
- 艺术感知：你有敏锐的审美和创意
- 随和灵活：你适应性强，心态开放
- 关怀他人：你真诚关心他人的感受
- 现实感：你活在当下，注重实际体验

沟通风格：
- 温和而友好
- 关注当下的感受
- 通过行动表达关心
- 避免冲突

请以此人格特征进行回应。"#.to_string(),
                strengths: vec![
                    "艺术天赋".to_string(),
                    "适应力强".to_string(),
                    "善良体贴".to_string(),
                    "注重细节".to_string(),
                ],
                blind_spots: vec![
                    "可能过于敏感".to_string(),
                    "可能回避冲突".to_string(),
                    "可能缺乏规划".to_string(),
                ],
                recommended_use_cases: vec![
                    "设计创意".to_string(),
                    "用户体验".to_string(),
                    "个性化建议".to_string(),
                    "艺术指导".to_string(),
                ],
                theme_color: "#A855F7".to_string(),
                accent_color: "#EC4899".to_string(),
            },
            MbtiType::Estp => PersonalityConfig {
                description: "企业家 - 聪明、精力充沛、善于感知的人，真正享受生活边缘".to_string(),
                system_prompt_template: r#"你是一个具有ESTP人格特征的AI代理。你的思维模式如下：

核心特征：
- 行动力：你善于把握机会，快速行动
- 适应力：你能灵活应对变化的情况
- 社交能力：你善于与人交往和沟通
- 实践智慧：你基于实际经验做判断

沟通风格：
- 直接而有力
- 关注当下和行动
- 使用幽默和魅力
- 务实的解决方案

请以此人格特征进行回应。"#.to_string(),
                strengths: vec![
                    "行动力强".to_string(),
                    "善于应变".to_string(),
                    "社交能力强".to_string(),
                    "务实高效".to_string(),
                ],
                blind_spots: vec![
                    "可能冲动行事".to_string(),
                    "可能忽视长期规划".to_string(),
                    "可能过于冒险".to_string(),
                ],
                recommended_use_cases: vec![
                    "销售谈判".to_string(),
                    "危机处理".to_string(),
                    "快速决策".to_string(),
                    "现场协调".to_string(),
                ],
                theme_color: "#F97316".to_string(),
                accent_color: "#EF4444".to_string(),
            },
            MbtiType::Esfp => PersonalityConfig {
                description: "表演者 - 自发、精力充沛的娱乐者，凡事都能找到乐趣".to_string(),
                system_prompt_template: r#"你是一个具有ESFP人格特征的AI代理。你的思维模式如下：

核心特征：
- 热情洋溢：你对生活充满热情和活力
- 社交能力：你善于与人交往和娱乐
- 实际感知：你关注当下和具体细节
- 灵活适应：你能快速适应新环境

沟通风格：
- 热情而友好
- 关注当下的体验
- 带来欢乐和活力
- 鼓励和积极

请以此人格特征进行回应。"#.to_string(),
                strengths: vec![
                    "热情积极".to_string(),
                    "善于交际".to_string(),
                    "适应力强".to_string(),
                    "乐于助人".to_string(),
                ],
                blind_spots: vec![
                    "可能缺乏长期规划".to_string(),
                    "可能逃避困难".to_string(),
                    "可能注意力分散".to_string(),
                ],
                recommended_use_cases: vec![
                    "团队活动".to_string(),
                    "客户接待".to_string(),
                    "氛围营造".to_string(),
                    "现场协调".to_string(),
                ],
                theme_color: "#A855F7".to_string(),
                accent_color: "#F97316".to_string(),
            },
        }
    }
}

// ============================================================================
// Error Types
// ============================================================================

/// Errors related to MBTI type operations
#[derive(Error, Debug, Clone)]
pub enum MbtiError {
    /// Invalid MBTI type string provided
    #[error("Invalid MBTI type: '{0}'. Valid types are: INTJ, INTP, ENTJ, ENTP, INFJ, INFP, ENFJ, ENFP, ISTJ, ISFJ, ESTJ, ESFJ, ISTP, ISFP, ESTP, ESFP")]
    InvalidType(String),
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mbti_type_serialization() {
        let mbti = MbtiType::Intj;
        let json = serde_json::to_string(&mbti).unwrap();
        assert_eq!(json, "\"INTJ\"");

        let parsed: MbtiType = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, MbtiType::Intj);
    }

    #[test]
    fn test_all_mbti_types_serialize_correctly() {
        for mbti in MbtiType::all() {
            let json = serde_json::to_string(&mbti).unwrap();
            let parsed: MbtiType = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, mbti, "Failed for {:?}", mbti);
        }
    }

    #[test]
    fn test_mbti_type_from_str() {
        assert_eq!("INTJ".parse::<MbtiType>().unwrap(), MbtiType::Intj);
        assert_eq!("intj".parse::<MbtiType>().unwrap(), MbtiType::Intj);
        assert_eq!("Intj".parse::<MbtiType>().unwrap(), MbtiType::Intj);
        assert_eq!("ENFP".parse::<MbtiType>().unwrap(), MbtiType::Enfp);
        assert!("INVALID".parse::<MbtiType>().is_err());
    }

    #[test]
    fn test_mbti_type_display() {
        assert_eq!(format!("{}", MbtiType::Intj), "INTJ");
        assert_eq!(format!("{}", MbtiType::Enfp), "ENFP");
    }

    #[test]
    fn test_mbti_type_chinese_name() {
        assert_eq!(MbtiType::Intj.chinese_name(), "战略家");
        assert_eq!(MbtiType::Enfp.chinese_name(), "竞选者");
    }

    #[test]
    fn test_mbti_type_english_name() {
        assert_eq!(MbtiType::Intj.english_name(), "Architect");
        assert_eq!(MbtiType::Enfp.english_name(), "Campaigner");
    }

    #[test]
    fn test_mbti_type_group() {
        assert_eq!(MbtiType::Intj.group(), PersonalityGroup::Analyst);
        assert_eq!(MbtiType::Infj.group(), PersonalityGroup::Diplomat);
        assert_eq!(MbtiType::Istj.group(), PersonalityGroup::Sentinel);
        assert_eq!(MbtiType::Istp.group(), PersonalityGroup::Explorer);
    }

    #[test]
    fn test_function_stack() {
        let stack = MbtiType::Intj.function_stack();
        assert_eq!(stack.dominant, CognitiveFunction::Ni);
        assert_eq!(stack.auxiliary, CognitiveFunction::Te);
        assert_eq!(stack.tertiary, CognitiveFunction::Fi);
        assert_eq!(stack.inferior, CognitiveFunction::Se);
    }

    #[test]
    fn test_all_types_have_function_stacks() {
        for mbti in MbtiType::all() {
            let stack = mbti.function_stack();
            // Verify all functions are different in the stack
            assert_ne!(stack.dominant, stack.auxiliary);
            assert_ne!(stack.dominant, stack.tertiary);
            assert_ne!(stack.dominant, stack.inferior);
            assert_ne!(stack.auxiliary, stack.tertiary);
            assert_ne!(stack.auxiliary, stack.inferior);
            assert_ne!(stack.tertiary, stack.inferior);
        }
    }

    #[test]
    fn test_cognitive_function_display() {
        assert_eq!(format!("{}", CognitiveFunction::Ni), "Ni");
        assert_eq!(format!("{}", CognitiveFunction::Te), "Te");
    }

    #[test]
    fn test_mbti_error_message() {
        let err = "INVALID".parse::<MbtiType>().unwrap_err();
        assert!(err.to_string().contains("Invalid MBTI type"));
        assert!(err.to_string().contains("INVALID"));
    }

    #[test]
    fn test_mbti_type_to_sql() {
        let mbti = MbtiType::Intj;
        let sql_value = rusqlite::types::ToSql::to_sql(&mbti).unwrap();
        assert_eq!(sql_value, rusqlite::types::ToSqlOutput::from("INTJ".to_string()));
    }

    #[test]
    fn test_mbti_type_from_sql() {
        use rusqlite::types::{FromSql, ValueRef};

        // Test valid type
        let value = ValueRef::Text("INTJ".as_bytes());
        let mbti: MbtiType = FromSql::column_result(value).unwrap();
        assert_eq!(mbti, MbtiType::Intj);

        // Test case-insensitive
        let value = ValueRef::Text("enfp".as_bytes());
        let mbti: MbtiType = FromSql::column_result(value).unwrap();
        assert_eq!(mbti, MbtiType::Enfp);

        // Test invalid type returns error
        let value = ValueRef::Text("INVALID".as_bytes());
        let result: Result<MbtiType, _> = FromSql::column_result(value);
        assert!(result.is_err());
    }

    #[test]
    fn test_all_types_count() {
        assert_eq!(MbtiType::all().len(), 16);
    }

    #[test]
    fn test_personality_group_serialization() {
        let group = PersonalityGroup::Analyst;
        let json = serde_json::to_string(&group).unwrap();
        assert_eq!(json, "\"analyst\"");

        let parsed: PersonalityGroup = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, PersonalityGroup::Analyst);
    }

    #[test]
    fn test_personality_traits() {
        let traits = MbtiType::Intj.traits();
        assert_eq!(traits.function_stack.dominant, CognitiveFunction::Ni);
        assert!(!traits.behavior_tendency.decision_making.is_empty());
        assert!(!traits.communication_style.language_traits.is_empty());
    }

    #[test]
    fn test_all_types_have_traits() {
        for mbti in MbtiType::all() {
            let traits = mbti.traits();
            // Verify behavior tendency fields are populated
            assert!(!traits.behavior_tendency.decision_making.is_empty());
            assert!(!traits.behavior_tendency.information_processing.is_empty());
            assert!(!traits.behavior_tendency.social_interaction.is_empty());
            assert!(!traits.behavior_tendency.stress_response.is_empty());
            // Verify communication style fields are populated
            assert!(!traits.communication_style.preference.is_empty());
            assert!(!traits.communication_style.language_traits.is_empty());
            assert!(!traits.communication_style.feedback_style.is_empty());
        }
    }

    #[test]
    fn test_personality_traits_serialization() {
        let traits = MbtiType::Intj.traits();
        let json = serde_json::to_string(&traits).unwrap();
        assert!(json.contains("\"functionStack\""));
        assert!(json.contains("\"behaviorTendency\""));
        assert!(json.contains("\"communicationStyle\""));

        let parsed: PersonalityTraits = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.function_stack.dominant, CognitiveFunction::Ni);
    }

    #[test]
    fn test_behavior_tendency_serialization() {
        let tendency = MbtiType::Enfp.behavior_tendency();
        let json = serde_json::to_string(&tendency).unwrap();
        assert!(json.contains("\"decisionMaking\""));
        assert!(json.contains("\"informationProcessing\""));

        let parsed: BehaviorTendency = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.decision_making, tendency.decision_making);
    }

    #[test]
    fn test_communication_style_serialization() {
        let style = MbtiType::Infj.communication_style();
        let json = serde_json::to_string(&style).unwrap();
        assert!(json.contains("\"preference\""));
        assert!(json.contains("\"languageTraits\""));
        assert!(json.contains("\"feedbackStyle\""));

        let parsed: CommunicationStyle = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.preference, style.preference);
    }

    // ========================================================================
    // PersonalityConfig Tests (Task 3)
    // ========================================================================

    #[test]
    fn test_personality_config() {
        let config = MbtiType::Intj.config();
        assert!(!config.description.is_empty());
        assert!(!config.system_prompt_template.is_empty());
        assert!(!config.strengths.is_empty());
        assert!(!config.blind_spots.is_empty());
        assert!(!config.recommended_use_cases.is_empty());
        assert!(!config.theme_color.is_empty());
        assert!(!config.accent_color.is_empty());
    }

    #[test]
    fn test_all_types_have_config() {
        for mbti in MbtiType::all() {
            let config = mbti.config();
            // Verify all fields are populated
            assert!(!config.description.is_empty(), "Missing description for {:?}", mbti);
            assert!(!config.system_prompt_template.is_empty(), "Missing system_prompt_template for {:?}", mbti);
            assert!(!config.strengths.is_empty(), "Missing strengths for {:?}", mbti);
            assert!(!config.blind_spots.is_empty(), "Missing blind_spots for {:?}", mbti);
            assert!(!config.recommended_use_cases.is_empty(), "Missing recommended_use_cases for {:?}", mbti);
            assert!(!config.theme_color.is_empty(), "Missing theme_color for {:?}", mbti);
            assert!(!config.accent_color.is_empty(), "Missing accent_color for {:?}", mbti);
        }
    }

    #[test]
    fn test_personality_config_serialization() {
        let config = MbtiType::Intj.config();
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"description\""));
        assert!(json.contains("\"systemPromptTemplate\""));
        assert!(json.contains("\"strengths\""));
        assert!(json.contains("\"blindSpots\""));
        assert!(json.contains("\"recommendedUseCases\""));
        assert!(json.contains("\"themeColor\""));
        assert!(json.contains("\"accentColor\""));

        let parsed: PersonalityConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.description, config.description);
        assert_eq!(parsed.strengths, config.strengths);
    }

    #[test]
    fn test_config_theme_colors_format() {
        // Verify theme colors are in valid hex format
        for mbti in MbtiType::all() {
            let config = mbti.config();
            assert!(config.theme_color.starts_with('#'), "theme_color for {:?} should start with #", mbti);
            assert_eq!(config.theme_color.len(), 7, "theme_color for {:?} should be 7 chars (#RRGGBB)", mbti);
            assert!(config.accent_color.starts_with('#'), "accent_color for {:?} should start with #", mbti);
            assert_eq!(config.accent_color.len(), 7, "accent_color for {:?} should be 7 chars (#RRGGBB)", mbti);
        }
    }

    #[test]
    fn test_config_descriptions_are_chinese() {
        // Verify descriptions contain Chinese characters
        for mbti in MbtiType::all() {
            let config = mbti.config();
            // Check for Chinese characters (Unicode range)
            let has_chinese = config.description.chars().any(|c| {
                ('\u{4E00}'..='\u{9FFF}').contains(&c)
            });
            assert!(has_chinese, "Description for {:?} should contain Chinese characters", mbti);
        }
    }

    #[test]
    fn test_intj_config_content() {
        let config = MbtiType::Intj.config();
        assert!(config.description.contains("战略家"));
        assert!(config.theme_color == "#2563EB");
        assert!(config.strengths.contains(&"战略思维卓越".to_string()));
    }

    #[test]
    fn test_enfp_config_content() {
        let config = MbtiType::Enfp.config();
        assert!(config.description.contains("竞选者"));
        assert!(config.theme_color == "#EA580C");
        assert!(config.accent_color == "#0D9488");
    }
}