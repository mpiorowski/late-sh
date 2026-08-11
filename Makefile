####################################################
# Docker
####################################################

# --- General (Docker/dev containers) ---
RUST_LOG ?= info,late_web=debug,late_ssh=debug,late_core=debug
CARGO_TARGET_DIR ?= /app/target
CARGO_INCREMENTAL ?= 0
CARGO_PROFILE_DEV_DEBUG ?= 1
INSTANCE ?= late                                            # Prefix for container names; bump (e.g. late2) for a parallel clone

# --- Environment profile ---
# Selects the late-ssh config.rs profile; ALL non-secret app config lives
# there. .env carries only compose plumbing, late-web/door-host settings,
# and secrets. Instance 2 runs the dev2 profile (see INSTANCE2_OVERRIDES).
LATE_ENV ?= dev

# --- SSH (host port mappings; must match the dev profile in late-ssh/src/config.rs) ---
LATE_SSH_PORT ?= 2222                                       # SSH host port mapping
LATE_API_PORT ?= 4001                                       # HTTP API host port mapping

# --- Database (credentials only; host/port/pool live in each app's config.rs) ---
LATE_DB_USER ?= postgres                                    # PostgreSQL user
LATE_DB_PASSWORD ?= postgres                                # PostgreSQL password
LATE_DB_NAME ?= postgres                                    # PostgreSQL database name
LATE_PG_HOST_PORT ?= 5433                                   # Host-side port mapped to postgres 5432

# --- Audio ---
LATE_ICECAST_HOST_PORT ?= 8000                              # Host-side port mapped to icecast 8000

# --- Voice ---
# Host-side ports for the local LiveKit dev container.
LATE_LIVEKIT_HOST_PORT ?= 7880
LATE_LIVEKIT_RTC_TCP_PORT ?= 7881
LATE_LIVEKIT_RTC_UDP_PORT ?= 7882
# Local LiveKit credentials (shared between late-ssh and the LiveKit container).
LATE_LIVEKIT_API_KEY ?= devkey
LATE_LIVEKIT_API_SECRET ?= secret

# --- IRC (host port mappings; must match the dev profile) ---
LATE_IRC_PORT ?= 6667
LATE_IRC_TLS_HOST_PORT ?= 6697

# --- Door games (shared secrets and host-pod settings; the late-ssh client
# side, hosts and enable flags, lives in late-ssh/src/config.rs) ---
LATE_REBELS_SECRET ?= $(shell openssl rand -hex 32 2>/dev/null || od -An -N32 -tx1 /dev/urandom | tr -d ' \n') # Shared secret seeding the derived rebels identity
LATE_NETHACK_PORT ?= 2323                                   # late-nethack SSH port
LATE_NETHACK_SECRET ?= $(shell openssl rand -hex 32 2>/dev/null || od -An -N32 -tx1 /dev/urandom | tr -d ' \n') # Shared secret authorizing late-ssh -> late-nethack
LATE_DOPEWARS_PORT ?= 2324                                  # late-dopewars SSH port
LATE_DOPEWARS_SECRET ?= $(shell openssl rand -hex 32 2>/dev/null || od -An -N32 -tx1 /dev/urandom | tr -d ' \n') # Shared secret authorizing late-ssh -> late-dopewars
LATE_DOPEWARS_SCORE_FILE ?= /tmp/late-dopewars.sco          # Shared high-score file on the dopewars host (a PVC path in prod)
LATE_CODEKEEP_PORT ?= 2328                                  # late-codekeep SSH port
LATE_CODEKEEP_SECRET ?= $(shell openssl rand -hex 32 2>/dev/null || od -An -N32 -tx1 /dev/urandom | tr -d ' \n') # Shared secret authorizing late-ssh -> late-codekeep
LATE_CODEKEEP_DATA_DIR ?= /var/lib/late-codekeep             # Per-account CodeKeep HOME roots
LATE_DCSS_PORT ?= 2325                                      # late-dcss SSH port
LATE_DCSS_SECRET ?= $(shell openssl rand -hex 32 2>/dev/null || od -An -N32 -tx1 /dev/urandom | tr -d ' \n') # Shared secret authorizing late-ssh -> late-dcss
LATE_USURPER_PORT ?= 2326                                   # late-usurper SSH port
LATE_USURPER_SECRET ?= $(shell openssl rand -hex 32 2>/dev/null || od -An -N32 -tx1 /dev/urandom | tr -d ' \n') # Shared secret authorizing late-ssh -> late-usurper
LATE_BROGUE_PORT ?= 2327                                    # late-brogue SSH port
LATE_BROGUE_SECRET ?= $(shell openssl rand -hex 32 2>/dev/null || od -An -N32 -tx1 /dev/urandom | tr -d ' \n') # Shared secret authorizing late-ssh -> late-brogue

# --- Web ---
LATE_WEB_PORT ?= 3000                                       # Web host port mapping (must match the dev profile in late-web/src/config.rs)
LATE_YOUTUBE_API_KEY ?=                                     # Optional dev opt-in for queue link validation

# --- AI (Gemini - used for @bot and @graybeard chat + URL extraction) ---
# The dev profile has AI enabled, so this key is required to boot; put a real
# key in .env.local.
LATE_AI_API_KEY ?=

####################################################
# Targets
####################################################

# App config does NOT live in .env: late-ssh reads LATE_ENV plus secrets and
# compiles everything else into its profile (late-ssh/src/config.rs). The
# lines below are compose port mappings, late-web and door-host settings,
# and shared dev secrets. Personal secrets (AI key) go in .env.local.
.PHONY: .env
.env:
	@echo "RUST_LOG=$(RUST_LOG)" > .env
	@echo "CARGO_TARGET_DIR=$(CARGO_TARGET_DIR)" >> .env
	@echo "CARGO_INCREMENTAL=$(CARGO_INCREMENTAL)" >> .env
	@echo "CARGO_PROFILE_DEV_DEBUG=$(CARGO_PROFILE_DEV_DEBUG)" >> .env
	@echo "INSTANCE=$(INSTANCE)" >> .env
	@echo "LATE_ENV=$(LATE_ENV)" >> .env
	@echo "LATE_SSH_PORT=$(LATE_SSH_PORT)" >> .env
	@echo "LATE_API_PORT=$(LATE_API_PORT)" >> .env
	@echo "LATE_DB_USER=$(LATE_DB_USER)" >> .env
	@echo "LATE_DB_PASSWORD=$(LATE_DB_PASSWORD)" >> .env
	@echo "LATE_DB_NAME=$(LATE_DB_NAME)" >> .env
	@echo "LATE_PG_HOST_PORT=$(LATE_PG_HOST_PORT)" >> .env
	@echo "LATE_ICECAST_HOST_PORT=$(LATE_ICECAST_HOST_PORT)" >> .env
	@echo "LATE_LIVEKIT_HOST_PORT=$(LATE_LIVEKIT_HOST_PORT)" >> .env
	@echo "LATE_LIVEKIT_RTC_TCP_PORT=$(LATE_LIVEKIT_RTC_TCP_PORT)" >> .env
	@echo "LATE_LIVEKIT_RTC_UDP_PORT=$(LATE_LIVEKIT_RTC_UDP_PORT)" >> .env
	@echo "LATE_LIVEKIT_API_KEY=$(LATE_LIVEKIT_API_KEY)" >> .env
	@echo "LATE_LIVEKIT_API_SECRET=$(LATE_LIVEKIT_API_SECRET)" >> .env
	@echo "LATE_IRC_PORT=$(LATE_IRC_PORT)" >> .env
	@echo "LATE_IRC_TLS_HOST_PORT=$(LATE_IRC_TLS_HOST_PORT)" >> .env
	@echo "LATE_REBELS_SECRET=$(LATE_REBELS_SECRET)" >> .env
	@echo "LATE_NETHACK_PORT=$(LATE_NETHACK_PORT)" >> .env
	@echo "LATE_NETHACK_SECRET=$(LATE_NETHACK_SECRET)" >> .env
	@echo "LATE_DOPEWARS_PORT=$(LATE_DOPEWARS_PORT)" >> .env
	@echo "LATE_DOPEWARS_SECRET=$(LATE_DOPEWARS_SECRET)" >> .env
	@echo "LATE_DOPEWARS_SCORE_FILE=$(LATE_DOPEWARS_SCORE_FILE)" >> .env
	@echo "LATE_CODEKEEP_PORT=$(LATE_CODEKEEP_PORT)" >> .env
	@echo "LATE_CODEKEEP_SECRET=$(LATE_CODEKEEP_SECRET)" >> .env
	@echo "LATE_CODEKEEP_DATA_DIR=$(LATE_CODEKEEP_DATA_DIR)" >> .env
	@echo "LATE_DCSS_PORT=$(LATE_DCSS_PORT)" >> .env
	@echo "LATE_DCSS_SECRET=$(LATE_DCSS_SECRET)" >> .env
	@echo "LATE_USURPER_PORT=$(LATE_USURPER_PORT)" >> .env
	@echo "LATE_USURPER_SECRET=$(LATE_USURPER_SECRET)" >> .env
	@echo "LATE_BROGUE_PORT=$(LATE_BROGUE_PORT)" >> .env
	@echo "LATE_BROGUE_SECRET=$(LATE_BROGUE_SECRET)" >> .env
	@echo "LATE_WEB_PORT=$(LATE_WEB_PORT)" >> .env
	@echo "LATE_YOUTUBE_API_KEY=$(LATE_YOUTUBE_API_KEY)" >> .env
	@echo "LATE_AI_API_KEY=$(LATE_AI_API_KEY)" >> .env

# Recipe for a parallel "instance 2" clone. Run from the second clone:
#   make start-instance2          # bring up the stack (foreground)
#   make .env-instance2           # just (re)generate .env without starting
# Only ports are overridden; URL/origin vars track the port defaults above.
INSTANCE2_OVERRIDES = \
  INSTANCE=late2 \
  LATE_ENV=dev2 \
  LATE_SSH_PORT=2223 \
  LATE_API_PORT=4001 \
  LATE_WEB_PORT=3001 \
  LATE_PG_HOST_PORT=5434 \
  LATE_ICECAST_HOST_PORT=8001 \
  LATE_LIQUIDSOAP_HOST_PORT=1235 \
  LATE_IRC_PORT=6668 \
  LATE_IRC_TLS_HOST_PORT=6698 \
  LATE_LIVEKIT_HOST_PORT=7883 \
  LATE_LIVEKIT_RTC_TCP_PORT=7884 \
  LATE_LIVEKIT_RTC_UDP_PORT=7885

CHECK_PACKAGES = -p late-cli -p late-core -p late-ssh -p late-web -p late-webview
CHECK_CARGO_ENV = CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
# Cap parallel compile/link jobs locally; 16-way builds spike past RAM on a
# swapless machine and freeze the desktop. CI overrides via CHECK_BUILD_JOBS.
CHECK_BUILD_JOBS ?= 8
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

# Fill the local Compose database with synthetic players so the Leaderboards
# page renders populated boards. Local development only: it owns the users whose
# fingerprints start with seed:leaderboard: and rewrites their stats on rerun.
.PHONY: seed-leaderboard
seed-leaderboard:
	scripts/seed_leaderboard_test_data.sh

.PHONY: check-db
check-db:
	$(CHECK_DB_START)

.PHONY: check-db-down
check-db-down:
	$(CHECK_DB_STOP)

# Targeted, memory-capped test run for LLM agents.
# Usage: make test-llm ARGS="-p late-ssh -E 'test(chat)'"
# MemoryHigh throttles the build before the desktop starves; MemoryMax kills
# the scope outright instead of freezing a swapless machine.
TEST_LLM_MEM_HIGH ?= 10G
TEST_LLM_MEM_MAX ?= 12G
.PHONY: test-llm
test-llm: .env
	@set -e; \
	trap 'status=$$?; $(CHECK_DB_STOP); exit $$status' EXIT; \
	$(CHECK_DB_START); \
	TEST_DATABASE_URL="$(CHECK_TEST_DATABASE_URL)" $(CHECK_CARGO_ENV) systemd-run --user --scope -q -p MemoryHigh=$(TEST_LLM_MEM_HIGH) -p MemoryMax=$(TEST_LLM_MEM_MAX) cargo nextest run --build-jobs $(CHECK_BUILD_JOBS) --no-fail-fast --failure-output final $(ARGS)

# Full pre-merge sweep, and the only place the otel feature is exercised:
# clippy + tests run the whole workspace WITH --features otel, so the real
# telemetry/metrics code (the config prod ships) is compiled and linted here.
# CI deliberately skips otel to stay cheap (see .github/workflows/ci.yml), so
# this is where otel breakage is caught before release. fmt stays scoped to
# first-party packages: `cargo fmt --all` also reaches vendored path deps like
# vendor/irc-proto, whose upstream style is not rustfmt-clean here.
.PHONY: check
check: .env
	@set -e; \
	trap 'status=$$?; $(CHECK_DB_STOP); exit $$status' EXIT; \
	$(CHECK_DB_START); \
	cargo fmt $(CHECK_PACKAGES) -- --check; \
	$(CHECK_CARGO_ENV) cargo clippy -j $(CHECK_BUILD_JOBS) --workspace --all-targets --features otel -- -D warnings; \
	TEST_DATABASE_URL="$(CHECK_TEST_DATABASE_URL)" $(CHECK_CARGO_ENV) cargo nextest run --build-jobs $(CHECK_BUILD_JOBS) --workspace --all-targets --no-fail-fast --failure-output final

start: .env keys
	docker compose -f docker-compose.yml up --build

.PHONY: start-amd64
start-amd64: .env keys
	@set -e; for image in \
		postgres:18 \
		libretime/icecast:2.4.4 \
		savonet/liquidsoap:v2.4.0 \
		livekit/livekit-server:latest; do \
		docker pull --platform linux/amd64 "$$image"; \
	done
	DOCKER_DEFAULT_PLATFORM=linux/amd64 docker compose -f docker-compose.yml up --build

startm: .env keys
	docker compose -f docker-compose.yml -f docker-compose.monitoring.yml up --build
down:
	docker compose -f docker-compose.yml -f docker-compose.monitoring.yml down
stop:
	docker compose -f docker-compose.yml -f docker-compose.monitoring.yml stop
remove: down
