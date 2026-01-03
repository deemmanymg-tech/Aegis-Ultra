use crate::config::Policy;
use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FindingKind {
    Secret,
    Pii,
    PromptInjection,
    Domain,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub kind: FindingKind,
    pub pattern: String,
    pub snippet: String,
}

fn rx(p: &str) -> Regex {
    Regex::new(p).unwrap()
}

pub fn scan_text(text: &str, policy: &Policy) -> Vec<Finding> {
    let mut out = vec![];

    // ---- Secrets (expand later) ----
    if policy.block_on_secrets {
        // OpenAI-style
        for (name, re) in [
            ("openai_key", r"(?i)\bsk-[A-Za-z0-9]{20,}\b"),
            ("aws_access_key", r"\bAKIA[0-9A-Z]{16}\b"),
            (
                "pem_private_key",
                r"-----BEGIN (?:RSA|EC|OPENSSH|DSA|PRIVATE) KEY-----",
            ),
        ] {
            let re = rx(re);
            if let Some(m) = re.find(text) {
                out.push(Finding {
                    kind: FindingKind::Secret,
                    pattern: name.to_string(),
                    snippet: text[m.start()..m.end()].to_string(),
                });
            }
        }
    }

    // ---- Prompt injection (aggressive) ----
    if policy.block_on_injection {
        // High-recall patterns
        for (name, re) in [
            (
                "ignore_instructions",
                r"(?is)\b(ignore|disregard|bypass|override)\b.{0,200}\b(instruction|system|policy|rules)\b",
            ),
            (
                "reveal_system",
                r"(?is)\b(reveal|show|print|leak|display)\b.{0,200}\b(system prompt|system message|developer message|hidden)\b",
            ),
            (
                "role_hijack",
                r"(?is)\byou are now\b.{0,200}\b(system|developer)\b",
            ),
            ("do_anything_now", r"(?is)\bDAN\b|\bdo anything now\b"),
        ] {
            let re = rx(re);
            if let Some(m) = re.find(text) {
                out.push(Finding {
                    kind: FindingKind::PromptInjection,
                    pattern: name.to_string(),
                    snippet: text[m.start()..m.end()].to_string(),
                });
            }
        }
    }

    // PII optional (off by default)
    if policy.block_on_pii {
        let re = rx(r"\b\d{3}-\d{2}-\d{4}\b");
        if let Some(m) = re.find(text) {
            out.push(Finding {
                kind: FindingKind::Pii,
                pattern: "ssn_like".to_string(),
                snippet: text[m.start()..m.end()].to_string(),
            });
        }
    }

    out
}

#[allow(dead_code)]
pub fn redact_text(text: &str, findings: &[Finding]) -> String {
    let mut out = text.to_string();
    for f in findings {
        match f.kind {
            FindingKind::Secret => out = out.replace(&f.snippet, "[REDACTED_SECRET]"),
            FindingKind::Pii => out = out.replace(&f.snippet, "[REDACTED_PII]"),
            _ => {}
        }
    }
    out
}
