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
const SYSTEM_PROMPT: &str = "Summarize this terminal output in 1-2 sentences.";

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
    // Note: /no_think disables Qwen3's reasoning mode for faster, more concise output
    let user_message = format!("$ {}\n{}\n/no_think", command, truncated_output);

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
    // Strip any <think>...</think> blocks (Qwen3 reasoning mode)
    let summary = strip_think_blocks(summary);

    summary
        .trim()
        // Remove any leading "Summary:" or similar
        .strip_prefix("Summary:")
        .unwrap_or(&summary)
        .trim()
        // Limit to reasonable length
        .chars()
        .take(500)
        .collect::<String>()
        .trim()
        .to_string()
}

/// Strip <think>...</think> blocks from Qwen3 output
fn strip_think_blocks(text: &str) -> String {
    let mut result = text.to_string();
    while let Some(start) = result.find("<think>") {
        if let Some(end) = result.find("</think>") {
            // Remove the think block including tags
            result = format!("{}{}", &result[..start], &result[end + 8..]);
        } else {
            // Unclosed think tag - remove from <think> to end
            result = result[..start].to_string();
            break;
        }
    }
    result
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
    fn test_strip_think_blocks() {
        assert_eq!(
            strip_think_blocks("<think>reasoning here</think>The answer"),
            "The answer"
        );
        assert_eq!(
            strip_think_blocks("Before <think>reasoning</think> after"),
            "Before  after"
        );
        assert_eq!(strip_think_blocks("No think blocks here"), "No think blocks here");
        // Unclosed think tag
        assert_eq!(strip_think_blocks("Start <think>incomplete"), "Start ");
    }

    #[test]
    fn test_clean_summary_strips_think_blocks() {
        assert_eq!(
            clean_summary("<think>Let me analyze this...</think>Build succeeded with 5 warnings."),
            "Build succeeded with 5 warnings."
        );
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
        assert!(SYSTEM_PROMPT.contains("Summarize"));
    }
}
