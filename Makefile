####################################################
# Docker
####################################################

# --- General (Docker/dev containers) ---
RUST_LOG ?= info,late_web=debug,late_ssh=debug,late_core=debug
CARGO_TARGET_DIR ?= /app/target
CARGO_INCREMENTAL ?= 0
CARGO_PROFILE_DEV_DEBUG ?= 1
INSTANCE ?= late                                            # Prefix for container names; bump (e.g. late2) for a parallel clone
LATE_UI_NEW_SHELL=1

# --- SSH ---
LATE_FORCE_ADMIN ?= 1
LATE_SSH_PORT ?= 2222                                       # SSH server listen port
LATE_API_PORT ?= 4001                                       # HTTP API listen port
LATE_SSH_OPEN ?= 1                                          # Allow connections without auth (1=open, 0=require key)
LATE_SSH_KEY_PATH ?= /app/server_key                        # Path to Ed25519 host key inside container
LATE_MAX_CONNS_GLOBAL ?= 10000                              # Max total concurrent SSH connections
LATE_MAX_CONNS_PER_IP ?= 3                                  # Max concurrent SSH connections from a single IP
LATE_SSH_IDLE_TIMEOUT ?= 3600                               # Disconnect idle SSH sessions after N seconds
LATE_FRAME_DROP_LOG_EVERY ?= 100                            # Log a warning every Nth dropped TUI frame
LATE_SSH_MAX_ATTEMPTS_PER_IP ?= 30                          # Max SSH connect attempts per IP before rate-limited
LATE_SSH_RATE_LIMIT_WINDOW_SECS ?= 60                       # Rolling window for SSH rate limiting
LATE_SSH_PROXY_PROTOCOL ?= 0                                # Parse PROXY protocol headers for real client IPs
LATE_SSH_PROXY_TRUSTED_CIDRS ?=                             # Comma-separated trusted proxy CIDRs (e.g. 10.42.0.0/16)
LATE_WS_PAIR_MAX_ATTEMPTS_PER_IP ?= 30                      # Max WebSocket pair requests per IP before rate-limited
LATE_WS_PAIR_RATE_LIMIT_WINDOW_SECS ?= 60                   # Rolling window for WS pair rate limiting
LATE_ALLOWED_ORIGINS ?= http://localhost:$(LATE_WEB_PORT)   # Comma-separated list of allowed CORS origins

# --- Database ---
LATE_DB_HOST ?= postgres                                    # PostgreSQL hostname (docker service name)
LATE_DB_PORT ?= 5432                                        # PostgreSQL port
LATE_DB_USER ?= postgres                                    # PostgreSQL user
LATE_DB_PASSWORD ?= postgres                                # PostgreSQL password
LATE_DB_NAME ?= postgres                                    # PostgreSQL database name
LATE_DB_POOL_SIZE ?= 16                                     # PostgreSQL connection pool size
LATE_PG_HOST_PORT ?= 5433                                   # Host-side port mapped to postgres 5432

# --- Audio ---
LATE_ICECAST_URL ?= http://icecast:8000                     # Icecast streaming server URL
LATE_ICECAST_HOST_PORT ?= 8000                              # Host-side port mapped to icecast 8000

# --- Voice ---
# Enable LiveKit-backed voice room control plane.
LATE_VOICE_ENABLED ?= 1
# Host-side ports for the local LiveKit dev container.
LATE_LIVEKIT_HOST_PORT ?= 7880
LATE_LIVEKIT_RTC_TCP_PORT ?= 7881
LATE_LIVEKIT_RTC_UDP_PORT ?= 7882
# Public LiveKit WebSocket URL sent to browsers and the CLI.
LATE_LIVEKIT_URL ?= ws://localhost:$(LATE_LIVEKIT_HOST_PORT)
# Local LiveKit credentials.
LATE_LIVEKIT_API_KEY ?= devkey
LATE_LIVEKIT_API_SECRET ?= secret
# Shared MVP voice room name.
LATE_VOICE_ROOM ?= late-voice

# --- IRC ---
# Enable the embedded IRC server in local dev.
LATE_IRC_ENABLED ?= 1
# Plaintext IRC listen port.
LATE_IRC_PORT ?= 6667

# --- Door games (Rebels in the Sky) ---
LATE_REBELS_ENABLED ?= 1                                    # Enable the Rebels in the Sky door game (1=on, 0=off)
LATE_REBELS_HOST ?= frittura.org                            # Rebels SSH server hostname to proxy to
LATE_REBELS_PORT ?= 3788                                    # Rebels SSH server port
LATE_REBELS_SECRET ?= $(shell openssl rand -hex 32 2>/dev/null || od -An -N32 -tx1 /dev/urandom | tr -d ' \n') # Shared secret seeding the derived rebels identity
LATE_NETHACK_ENABLED ?= 1

# --- Web ---
LATE_WEB_PORT ?= 3000                                       # Web server listen port
LATE_WEB_URL ?= http://localhost:$(LATE_WEB_PORT)           # Public web URL (used by SSH server)
LATE_SSH_INTERNAL_URL ?= http://service-ssh:$(LATE_API_PORT) # Internal SSH API URL (used by web server)
LATE_SSH_PUBLIC_URL ?= localhost:$(LATE_API_PORT)           # Public SSH API URL (used by browser for WS)
LATE_AUDIO_URL ?= http://icecast:8000                       # Upstream audio URL used by late-web /stream proxy
LATE_WEB_TUNNEL_TOKEN ?= dev-web-tunnel                     # Local-only shared token for /play web terminal
LATE_YOUTUBE_API_KEY ?=

# --- AI (Gemini - used for @bot and @graybeard chat + URL extraction) ---
LATE_AI_ENABLED ?= 1                                        # Enable AI-powered features
LATE_AI_API_KEY ?=                                              # Gemini API key for AI features
LATE_AI_MODEL ?= gemini-3.1-pro-preview                     # Gemini model to use

# --- Files / uploads (optional; blank disables uploads) ---
LATE_FILES_S3_ENDPOINT ?= https://8ecfba101ed3834cf19fd86e68fc325b.r2.cloudflarestorage.com # S3/R2 endpoint URL
LATE_FILES_S3_BUCKET ?= late-sh-r-files                     								# S3/R2 bucket for uploaded files
LATE_FILES_PUBLIC_BASE_URL ?= https://files.late.sh                               			# Public base URL, e.g. https://files.late.sh
LATE_FILES_S3_REGION ?= auto                                								# Cloudflare R2 signing region
LATE_FILES_MAX_UPLOAD_BYTES ?= 10485760                     								# Max image upload size
LATE_FILES_S3_ACCESS_KEY_ID ?=  								                            # S3/R2 access key ID
LATE_FILES_S3_SECRET_ACCESS_KEY ?=  								                        # S3/R2 secret access key

####################################################
# Targets
####################################################

.PHONY: .env
.env:
	@echo "RUST_LOG=$(RUST_LOG)" > .env
	@echo "CARGO_TARGET_DIR=$(CARGO_TARGET_DIR)" >> .env
	@echo "CARGO_INCREMENTAL=$(CARGO_INCREMENTAL)" >> .env
	@echo "CARGO_PROFILE_DEV_DEBUG=$(CARGO_PROFILE_DEV_DEBUG)" >> .env
	@echo "INSTANCE=$(INSTANCE)" >> .env
	@echo "LATE_UI_NEW_SHELL=$(LATE_UI_NEW_SHELL)" >> .env
	@echo "LATE_FORCE_ADMIN=$(LATE_FORCE_ADMIN)" >> .env
	@echo "LATE_SSH_PORT=$(LATE_SSH_PORT)" >> .env
	@echo "LATE_API_PORT=$(LATE_API_PORT)" >> .env
	@echo "LATE_SSH_OPEN=$(LATE_SSH_OPEN)" >> .env
	@echo "LATE_SSH_KEY_PATH=$(LATE_SSH_KEY_PATH)" >> .env
	@echo "LATE_MAX_CONNS_GLOBAL=$(LATE_MAX_CONNS_GLOBAL)" >> .env
	@echo "LATE_MAX_CONNS_PER_IP=$(LATE_MAX_CONNS_PER_IP)" >> .env
	@echo "LATE_SSH_IDLE_TIMEOUT=$(LATE_SSH_IDLE_TIMEOUT)" >> .env
	@echo "LATE_FRAME_DROP_LOG_EVERY=$(LATE_FRAME_DROP_LOG_EVERY)" >> .env
	@echo "LATE_SSH_MAX_ATTEMPTS_PER_IP=$(LATE_SSH_MAX_ATTEMPTS_PER_IP)" >> .env
	@echo "LATE_SSH_RATE_LIMIT_WINDOW_SECS=$(LATE_SSH_RATE_LIMIT_WINDOW_SECS)" >> .env
	@echo "LATE_SSH_PROXY_PROTOCOL=$(LATE_SSH_PROXY_PROTOCOL)" >> .env
	@echo "LATE_SSH_PROXY_TRUSTED_CIDRS=$(LATE_SSH_PROXY_TRUSTED_CIDRS)" >> .env
	@echo "LATE_WS_PAIR_MAX_ATTEMPTS_PER_IP=$(LATE_WS_PAIR_MAX_ATTEMPTS_PER_IP)" >> .env
	@echo "LATE_WS_PAIR_RATE_LIMIT_WINDOW_SECS=$(LATE_WS_PAIR_RATE_LIMIT_WINDOW_SECS)" >> .env
	@echo "LATE_ALLOWED_ORIGINS=$(LATE_ALLOWED_ORIGINS)" >> .env
	@echo "LATE_DB_HOST=$(LATE_DB_HOST)" >> .env
	@echo "LATE_DB_PORT=$(LATE_DB_PORT)" >> .env
	@echo "LATE_DB_USER=$(LATE_DB_USER)" >> .env
	@echo "LATE_DB_PASSWORD=$(LATE_DB_PASSWORD)" >> .env
	@echo "LATE_DB_NAME=$(LATE_DB_NAME)" >> .env
	@echo "LATE_DB_POOL_SIZE=$(LATE_DB_POOL_SIZE)" >> .env
	@echo "LATE_PG_HOST_PORT=$(LATE_PG_HOST_PORT)" >> .env
	@echo "LATE_ICECAST_URL=$(LATE_ICECAST_URL)" >> .env
	@echo "LATE_ICECAST_HOST_PORT=$(LATE_ICECAST_HOST_PORT)" >> .env
	@echo "LATE_VOICE_ENABLED=$(LATE_VOICE_ENABLED)" >> .env
	@echo "LATE_LIVEKIT_URL=$(LATE_LIVEKIT_URL)" >> .env
	@echo "LATE_LIVEKIT_HOST_PORT=$(LATE_LIVEKIT_HOST_PORT)" >> .env
	@echo "LATE_LIVEKIT_RTC_TCP_PORT=$(LATE_LIVEKIT_RTC_TCP_PORT)" >> .env
	@echo "LATE_LIVEKIT_RTC_UDP_PORT=$(LATE_LIVEKIT_RTC_UDP_PORT)" >> .env
	@echo "LATE_LIVEKIT_API_KEY=$(LATE_LIVEKIT_API_KEY)" >> .env
	@echo "LATE_LIVEKIT_API_SECRET=$(LATE_LIVEKIT_API_SECRET)" >> .env
	@echo "LATE_VOICE_ROOM=$(LATE_VOICE_ROOM)" >> .env
	@echo "LATE_IRC_ENABLED=$(LATE_IRC_ENABLED)" >> .env
	@echo "LATE_IRC_PORT=$(LATE_IRC_PORT)" >> .env
	@echo "" >> .env
	@echo "# Optional IRC TLS/tuning overrides:" >> .env
	@echo "# LATE_IRC_TLS_CERT=/path/to/fullchain.pem" >> .env
	@echo "# LATE_IRC_TLS_KEY=/path/to/privkey.pem" >> .env
	@echo "# LATE_IRC_MAX_CONNS_GLOBAL=200" >> .env
	@echo "# LATE_IRC_MAX_CONNS_PER_USER=3" >> .env
	@echo "# LATE_IRC_MAX_AUTH_FAILURES_PER_IP=20" >> .env
	@echo "# LATE_IRC_AUTH_FAILURE_WINDOW_SECS=300" >> .env
	@echo "LATE_REBELS_ENABLED=$(LATE_REBELS_ENABLED)" >> .env
	@echo "LATE_REBELS_HOST=$(LATE_REBELS_HOST)" >> .env
	@echo "LATE_REBELS_PORT=$(LATE_REBELS_PORT)" >> .env
	@echo "LATE_REBELS_SECRET=$(LATE_REBELS_SECRET)" >> .env
	@echo "LATE_NETHACK_ENABLED=$(LATE_NETHACK_ENABLED)" >> .env
	@echo "LATE_WEB_PORT=$(LATE_WEB_PORT)" >> .env
	@echo "LATE_WEB_URL=$(LATE_WEB_URL)" >> .env
	@echo "LATE_SSH_INTERNAL_URL=$(LATE_SSH_INTERNAL_URL)" >> .env
	@echo "LATE_SSH_PUBLIC_URL=$(LATE_SSH_PUBLIC_URL)" >> .env
	@echo "LATE_AUDIO_URL=$(LATE_AUDIO_URL)" >> .env
	@echo "LATE_WEB_TUNNEL_TOKEN=$(LATE_WEB_TUNNEL_TOKEN)" >> .env
	@echo "LATE_YOUTUBE_API_KEY=$(LATE_YOUTUBE_API_KEY)" >> .env
	@echo "LATE_AI_ENABLED=$(LATE_AI_ENABLED)" >> .env
	@echo "LATE_AI_API_KEY=$(LATE_AI_API_KEY)" >> .env
	@echo "LATE_AI_MODEL=$(LATE_AI_MODEL)" >> .env
	@echo "LATE_FILES_S3_ENDPOINT=$(LATE_FILES_S3_ENDPOINT)" >> .env
	@echo "LATE_FILES_S3_BUCKET=$(LATE_FILES_S3_BUCKET)" >> .env
	@echo "LATE_FILES_PUBLIC_BASE_URL=$(LATE_FILES_PUBLIC_BASE_URL)" >> .env
	@echo "LATE_FILES_S3_REGION=$(LATE_FILES_S3_REGION)" >> .env
	@echo "LATE_FILES_S3_ACCESS_KEY_ID=$(LATE_FILES_S3_ACCESS_KEY_ID)" >> .env
	@echo "LATE_FILES_S3_SECRET_ACCESS_KEY=$(LATE_FILES_S3_SECRET_ACCESS_KEY)" >> .env
	@echo "LATE_FILES_MAX_UPLOAD_BYTES=$(LATE_FILES_MAX_UPLOAD_BYTES)" >> .env

# Recipe for a parallel "instance 2" clone. Run from the second clone:
#   make start-instance2          # bring up the stack (foreground)
#   make .env-instance2           # just (re)generate .env without starting
# Only ports are overridden; URL/origin vars track the port defaults above.
INSTANCE2_OVERRIDES = \
  INSTANCE=late2 \
  LATE_SSH_PORT=2223 \
  LATE_API_PORT=4001 \
  LATE_WEB_PORT=3001 \
  LATE_PG_HOST_PORT=5434 \
  LATE_ICECAST_HOST_PORT=8001 \
  LATE_LIQUIDSOAP_HOST_PORT=1235 \
  LATE_IRC_PORT=6668 \
  LATE_LIVEKIT_HOST_PORT=7883 \
  LATE_LIVEKIT_RTC_TCP_PORT=7884 \
  LATE_LIVEKIT_RTC_UDP_PORT=7885

CHECK_PACKAGES = -p late-cli -p late-core -p late-ssh -p late-web
CHECK_CARGO_ENV = CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
CHECK_INSTANCE ?= late-check
CHECK_PG_HOST_PORT ?= 55433
CHECK_COMPOSE = CHECK_PG_HOST_PORT=$(CHECK_PG_HOST_PORT) docker compose -p $(CHECK_INSTANCE) -f docker-compose.check.yml
CHECK_TEST_DATABASE_URL ?= host=127.0.0.1 port=$(CHECK_PG_HOST_PORT) user=postgres password=postgres dbname=postgres
CHECK_DB_STOP = $(CHECK_COMPOSE) down -v --remove-orphans
CHECK_DB_RESET = $(CHECK_DB_STOP) >/dev/null 2>&1 || true
CHECK_DB_START = $(CHECK_DB_RESET); $(CHECK_COMPOSE) up -d --wait postgres

.PHONY: .env-instance2
.env-instance2:
	@$(MAKE) .env $(INSTANCE2_OVERRIDES)

.PHONY: start-instance2
start-instance2:
	@$(MAKE) start $(INSTANCE2_OVERRIDES)

.PHONY: keys
keys:
	@if [ ! -f server_key ]; then ssh-keygen -t ed25519 -f server_key -N "" -q; fi

.PHONY: check-db
check-db:
	$(CHECK_DB_START)

.PHONY: check-db-down
check-db-down:
	$(CHECK_DB_STOP)

.PHONY: check
check: .env
	@set -e; \
	trap 'status=$$?; $(CHECK_DB_STOP); exit $$status' EXIT; \
	$(CHECK_DB_START); \
	cargo fmt $(CHECK_PACKAGES) -- --check; \
	$(CHECK_CARGO_ENV) cargo clippy $(CHECK_PACKAGES) --all-targets --no-deps -- -D warnings; \
	TEST_DATABASE_URL="$(CHECK_TEST_DATABASE_URL)" $(CHECK_CARGO_ENV) cargo nextest run $(CHECK_PACKAGES) --all-targets --no-fail-fast

.PHONY: checkci
checkci: .env
	@set -e; \
	trap 'status=$$?; $(CHECK_DB_STOP); exit $$status' EXIT; \
	$(CHECK_DB_START); \
	cargo fmt --all -- --check; \
	$(CHECK_CARGO_ENV) cargo clippy --workspace --all-targets --features otel -- -D warnings; \
	TEST_DATABASE_URL="$(CHECK_TEST_DATABASE_URL)" $(CHECK_CARGO_ENV) cargo nextest run --workspace --all-targets

start: .env keys
	docker compose -f docker-compose.yml up --build

startm: .env keys
	docker compose -f docker-compose.yml -f docker-compose.monitoring.yml up --build
down:
	docker compose -f docker-compose.yml -f docker-compose.monitoring.yml down
stop:
	docker ps -aq | xargs -r docker stop
remove:
	docker ps -aq | xargs -r docker rm -f
