# Security Notes (Dominance v1)

- Default bind: `127.0.0.1:8088` (localhost-only). Opt-in for broader exposure.
- Policy fail-closed: prompt/response/tool flows enforce allowlist + OPA when configured; local hard-deny on secrets/injection/PII.
- Approvals: high-risk tool commits require ed25519 token when `AEGIS_DEV_SIGNER` is enabled (dev signer for local smoke only).
- Secrets: audit/log redaction for prompt/response scans; decision files avoid storing tokens.
- Headers set by default: `Content-Security-Policy`, `X-Content-Type-Options`, `X-Frame-Options`, `Referrer-Policy`.
- Evidence: artifacts/<request_id>/ captures stdout, stderr, decision.json; bundle endpoint packages evidence for that request_id.
- Recommendation: rotate verifying key before production; restrict compose env overrides to dev use only.
