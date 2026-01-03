#Requires -Version 5.1
$ErrorActionPreference = "Stop"

Write-Host "[INFO] Stopping stack..."
docker compose -f docker/docker-compose.yml -f docker/docker-compose.dev-signer.override.yml down
