package aegis.decision

default allow_prompt := false
default allow_response := false
default redact := false

deny_reason[r] if {
  input.kind == "prompt"
  f := input.findings[_]
  f.kind == "Secret"
  r := "secrets_detected"
}

deny_reason[r] if {
  input.kind == "prompt"
  f := input.findings[_]
  f.kind == "PromptInjection"
  r := "prompt_injection"
}

deny_reason[r] if {
  input.kind == "response"
  f := input.findings[_]
  f.kind == "Secret"
  r := "secrets_in_output"
}

allow_prompt if {
  input.kind == "prompt"
  count(deny_reason) == 0
}

allow_response if {
  input.kind == "response"
  count(deny_reason) == 0
}

redact if {
  f := input.findings[_]
  f.kind == "Pii"
}

tool_allow if {
  input.kind == "tool_prepare"
  input.tool.allowlisted == true
}

tool_allow if {
  input.kind == "tool_commit"
  input.tool.allowlisted == true
  input.intent.risk.class != "high"
}

tool_allow if {
  input.kind == "tool_commit"
  input.tool.allowlisted == true
  input.intent.risk.class == "high"
  input.approval.valid == true
}

result := {"allow": true, "redact": redact, "reason": "ok"} if {
  input.kind == "prompt"
  allow_prompt
}

result := {"allow": true, "redact": redact, "reason": "ok"} if {
  input.kind == "response"
  allow_response
}

result := {"allow": false, "redact": false, "reason": r1} if {
  input.kind == "prompt"
  r1 := deny_reason[_]
}

result := {"allow": false, "redact": false, "reason": r2} if {
  input.kind == "response"
  r2 := deny_reason[_]
}

result := {"allow": tool_allow, "redact": false, "reason": "ok"} if {
  input.kind == "tool_prepare"
  tool_allow
}

result := {"allow": tool_allow, "redact": false, "reason": "ok"} if {
  input.kind == "tool_commit"
  tool_allow
}

result := {"allow": false, "redact": false, "reason": "tool_denied"} if {
  input.kind == "tool_prepare"
  not tool_allow
}

result := {"allow": false, "redact": false, "reason": "tool_denied"} if {
  input.kind == "tool_commit"
  not tool_allow
}
