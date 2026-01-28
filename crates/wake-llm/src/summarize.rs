//! Command output summarization

use crate::model::Model;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SummarizeError {
    #[error("Model inference failed: {0}")]
    Inference(String),
    #[error("Output too short to summarize")]
    TooShort,
}

/// System prompt for summarization
const SYSTEM_PROMPT: &str = r#"You are a terminal output summarizer. Given a command and its output, provide a brief, informative summary in 1-2 sentences. Focus on:
- What happened (success/failure)
- Key results or changes
- Any errors or warnings

Be concise and technical. Do not include the command itself in the summary."#;

/// Minimum output length (bytes) worth summarizing
pub const MIN_OUTPUT_BYTES: usize = 100;

/// Maximum output to send to the model (to avoid token limits)
const MAX_OUTPUT_CHARS: usize = 4000;

/// Summarize command output using the model
pub fn summarize(model: &Model, command: &str, output: &str) -> Result<String, SummarizeError> {
    // Skip very short outputs
    if output.len() < MIN_OUTPUT_BYTES {
        return Err(SummarizeError::TooShort);
    }

    // Truncate output if too long
    let truncated_output = if output.len() > MAX_OUTPUT_CHARS {
        let half = MAX_OUTPUT_CHARS / 2;
        format!(
            "{}\n\n[... {} chars omitted ...]\n\n{}",
            &output[..half],
            output.len() - MAX_OUTPUT_CHARS,
            &output[output.len() - half..]
        )
    } else {
        output.to_string()
    };

    // Build the user message
    let user_message = format!("Command: {}\n\nOutput:\n```\n{}\n```", command, truncated_output);

    // Generate summary
    let summary = model
        .generate(SYSTEM_PROMPT, &user_message)
        .map_err(|e| SummarizeError::Inference(e.to_string()))?;

    // Clean up the summary
    let summary = clean_summary(&summary);

    Ok(summary)
}

/// Clean up the generated summary
fn clean_summary(summary: &str) -> String {
    summary
        .trim()
        // Remove any leading "Summary:" or similar
        .strip_prefix("Summary:")
        .unwrap_or(summary)
        .trim()
        // Limit to reasonable length
        .chars()
        .take(500)
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_summary_with_prefix() {
        assert_eq!(clean_summary("Summary: Test output"), "Test output");
    }

    #[test]
    fn test_clean_summary_whitespace() {
        assert_eq!(clean_summary("  Test output  "), "Test output");
        assert_eq!(clean_summary("\n\nTest output\n\n"), "Test output");
        assert_eq!(clean_summary("\t  Test  \t"), "Test");
    }

    #[test]
    fn test_clean_summary_empty() {
        assert_eq!(clean_summary(""), "");
        assert_eq!(clean_summary("   "), "");
    }

    #[test]
    fn test_clean_summary_truncates_long_text() {
        let long_text = "x".repeat(600);
        let cleaned = clean_summary(&long_text);
        assert!(cleaned.len() <= 500);
    }

    #[test]
    fn test_clean_summary_preserves_normal_text() {
        let text = "Build completed successfully. 42 tests passed, 0 failed.";
        assert_eq!(clean_summary(text), text);
    }

    #[test]
    fn test_min_output_bytes_constant() {
        assert_eq!(MIN_OUTPUT_BYTES, 100);
    }

    #[test]
    fn test_max_output_chars_constant() {
        assert_eq!(MAX_OUTPUT_CHARS, 4000);
    }

    #[test]
    fn test_system_prompt_not_empty() {
        #[allow(clippy::const_is_empty)]
        {
            assert!(!SYSTEM_PROMPT.is_empty());
        }
        assert!(SYSTEM_PROMPT.contains("summarizer"));
    }
}
