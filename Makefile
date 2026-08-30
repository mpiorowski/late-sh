####################################################
# Docker
####################################################

# Environment files: no config lives in this Makefile. .env is a verbatim
# copy of a committed template (.env.dev, or .env.dev2 for a parallel
# instance 2 clone), refreshed on every run; edit the template, not .env.
# Personal secrets and opt-ins go in .env.local (gitignored), which compose
# loads after .env, so it wins on duplicate keys. All non-secret app config
# is compiled into the profiles in late-ssh/src/config.rs.
ENV_TEMPLATE = .env.dev

####################################################
# Targets
####################################################

.PHONY: .env
.env:
	@cp $(ENV_TEMPLATE) .env

# Recipe for a parallel "instance 2" clone. Run from the second clone:
#   make start-instance2          # bring up the stack (foreground)
#   make .env-instance2           # just (re)generate .env without starting

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
	@$(MAKE) .env ENV_TEMPLATE=.env.dev2

.PHONY: start-instance2
start-instance2:
	@$(MAKE) start ENV_TEMPLATE=.env.dev2

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

# The Lateania battle arena: every balance contract and both report parts
# (late-ssh/target/lateania-arena*.md). Thousands of real engine fights, so
# it is not part of the suite; run it when combat, classes, gear, or bosses
# change. Serial (-j1): the report tests each need most of a CPU-minute budget.
.PHONY: arena
arena:
	$(MAKE) test-llm ARGS="-p late-ssh --run-ignored all -j1 -E 'test(lateania::svc::arena)'"

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
