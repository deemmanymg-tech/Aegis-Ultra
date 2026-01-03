# Aegis Ultra – Quickstart

1) Build + start (Docker Compose, includes OPA):
```
scripts/BUILD_ALL.ps1
docker compose -f docker/docker-compose.yml -f docker/docker-compose.dev-signer.override.yml up --build -d
```

2) Health + UI:
```
Invoke-RestMethod http://127.0.0.1:8088/healthz
# then open http://127.0.0.1:8088/ in a browser
```

3) Smoke + ship verification:
```
powershell -ExecutionPolicy Bypass -File scripts/SMOKE_DOMINANCE.ps1
powershell -ExecutionPolicy Bypass -File scripts/SHIP_VERIFY_ALL.ps1 -BaseUrl http://127.0.0.1:8088
```

4) Evidence:
- artifacts/<request_id>/… (stdout, stderr, decision.json)
- bundle ZIP from `SMOKE_DOMINANCE` (saved to %TEMP%\bundle_smoke.zip)

Default bind is localhost (127.0.0.1:8088). Dev signer override lives in `docker/docker-compose.dev-signer.override.yml`.
