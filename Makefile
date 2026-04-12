####################################################
# Docker
####################################################

# --- General (Docker/dev containers) ---
RUST_LOG ?= info,late_web=debug,late_ssh=debug,late_core=debug
CARGO_TARGET_DIR ?= /app/target

# --- SSH ---
LATE_SSH_PORT ?= 2222                                       # SSH server listen port
LATE_API_PORT ?= 4000                                       # HTTP API listen port
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
LATE_ALLOWED_ORIGINS ?= http://localhost:3000               # Comma-separated list of allowed CORS origins

# --- Database ---
LATE_DB_HOST ?= postgres                                    # PostgreSQL hostname (docker service name)
LATE_DB_PORT ?= 5432                                        # PostgreSQL port
LATE_DB_USER ?= postgres                                    # PostgreSQL user
LATE_DB_PASSWORD ?= postgres                                # PostgreSQL password
LATE_DB_NAME ?= postgres                                    # PostgreSQL database name
LATE_DB_POOL_SIZE ?= 16                                     # PostgreSQL connection pool size

# --- Audio ---
LATE_ICECAST_URL ?= http://icecast:8000                     # Icecast streaming server URL
LATE_LIQUIDSOAP_ADDR ?= liquidsoap:1234                     # Liquidsoap telnet address for vibe switching

# --- Web ---
LATE_WEB_PORT ?= 3001                                       # Web server listen port
LATE_WEB_URL ?= http://localhost:3001                       # Public web URL (used by SSH server)
LATE_SSH_INTERNAL_URL ?= http://service-ssh:4000            # Internal SSH API URL (used by web server)
LATE_SSH_PUBLIC_URL ?= localhost:4000                       # Public SSH API URL (used by browser for WS)
LATE_AUDIO_URL ?= http://icecast:8000                       # Upstream audio URL used by late-web /stream proxy

# --- Vote ---
LATE_VOTE_SWITCH_INTERVAL_SECS ?= 3600                      # Duration of each vote round (60 min)

# --- AI (Gemini - used for @bot and @graybeard chat + URL extraction) ---
LATE_AI_ENABLED ?= 1                                        # Enable AI-powered features
LATE_AI_API_KEY ?=                                              # Gemini API key for AI features
LATE_AI_MODEL ?= gemini-3.1-pro-preview                     # Gemini model to use

####################################################
# Targets
####################################################

# All vars above are written to .env, docker-compose reads it via env_file
.PHONY: .env
.env:
	@echo "RUST_LOG=$(RUST_LOG)" > .env
	@echo "CARGO_TARGET_DIR=$(CARGO_TARGET_DIR)" >> .env
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
	@echo "LATE_ICECAST_URL=$(LATE_ICECAST_URL)" >> .env
	@echo "LATE_LIQUIDSOAP_ADDR=$(LATE_LIQUIDSOAP_ADDR)" >> .env
	@echo "LATE_WEB_PORT=$(LATE_WEB_PORT)" >> .env
	@echo "LATE_WEB_URL=$(LATE_WEB_URL)" >> .env
	@echo "LATE_SSH_INTERNAL_URL=$(LATE_SSH_INTERNAL_URL)" >> .env
	@echo "LATE_SSH_PUBLIC_URL=$(LATE_SSH_PUBLIC_URL)" >> .env
	@echo "LATE_AUDIO_URL=$(LATE_AUDIO_URL)" >> .env
	@echo "LATE_VOTE_SWITCH_INTERVAL_SECS=$(LATE_VOTE_SWITCH_INTERVAL_SECS)" >> .env
	@echo "LATE_AI_ENABLED=$(LATE_AI_ENABLED)" >> .env
	@echo "LATE_AI_API_KEY=$(LATE_AI_API_KEY)" >> .env
	@echo "LATE_AI_MODEL=$(LATE_AI_MODEL)" >> .env

.PHONY: keys
keys:
	@if [ ! -f server_key ]; then ssh-keygen -t ed25519 -f server_key -N "" -q; fi

check:
	cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo nextest run --workspace --all-targets

start: .env keys
	docker compose -f docker-compose.yml up
startm: .env keys
	docker compose -f docker-compose.yml -f docker-compose.monitoring.yml up --build
down:
	docker compose -f docker-compose.yml -f docker-compose.monitoring.yml down
stop:
	docker ps -aq | xargs -r docker stop
remove:
	docker ps -aq | xargs -r docker rm -f
