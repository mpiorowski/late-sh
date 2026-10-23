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

CHECK_PACKAGES = -p late-cli -p late-core -p late-ssh -p late-web -p late-webview -p late-zork
# `env -u MAKEFLAGS -u MFLAGS`: make exports command-line variables (the
# `ARGS="..."` in `make test-llm ARGS=...`) through MAKEFLAGS, cargo forwards
# it to build scripts, and tikv-jemalloc-sys's nested make then parses ARGS'
# words as targets and dies on a fresh build. Cargo gives build scripts its
# own jobserver, so dropping make's costs no parallelism.
CHECK_CARGO_ENV = env -u MAKEFLAGS -u MFLAGS CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
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

# Load a long fake edition for newspaper scrolling checks in the local TUI.
PAPER_PARAGRAPHS ?= 100
.PHONY: seed-paper
seed-paper:
	scripts/seed_paper_test_data.sh "$(PAPER_PARAGRAPHS)"

# IRC history fixtures for an existing local account.
.PHONY: seed-irc-history
seed-irc-history:
	scripts/seed_irc_history.sh "$(USERNAME)"

# Artboard gallery fixtures: accounts, pieces, applause, and the on-connect splash.
ART_SPLASH_PIECE ?= 1
.PHONY: seed-artboard
seed-artboard:
	scripts/seed_artboard_test_data.sh "$(ART_SPLASH_PIECE)"

.PHONY: check-db
check-db:
	$(CHECK_DB_START)

.PHONY: check-db-down
check-db-down:
	$(CHECK_DB_STOP)

# Targeted, memory-capped test run for LLM agents.
# Usage: make test-llm ARGS="-p late-ssh --lib -E 'test(chat)'"
# (`--lib` on a crate with a lib and a main: without it nextest builds the lib
# a second time for the binary's empty test target, doubling the heaviest
# compile.)
# The run is a scope in background.slice, whose CPU weight (30, to app.slice's
# 100) lets the browser, audio and other sessions win the CPU. Its memory cap
# follows what is free at launch: MemAvailable less TEST_LLM_HEADROOM_G, never
# above TEST_LLM_MEM_MAX_G. A fixed cap larger than what is free never trips
# on a loaded swapless machine, and the global OOM killer takes the browser
# instead. MemoryHigh throttles the build 2G below the cap; MemoryMax kills the
# scope, not the desktop. Under TEST_LLM_MEM_MIN_G the run refuses to start:
# the late-ssh test build's rustc passes 8G, and with no swap a MemoryHigh
# under that stalls it for hours instead of failing.
TEST_LLM_MEM_MAX_G ?= 12
TEST_LLM_HEADROOM_G ?= 3
TEST_LLM_MEM_MIN_G ?= 11
.PHONY: test-llm
test-llm: .env
	@set -e; \
	avail_g=$$(awk '/^MemAvailable:/ {print int($$2 / 1048576)}' /proc/meminfo); \
	max_g=$$((avail_g - $(TEST_LLM_HEADROOM_G))); \
	if [ $$max_g -gt $(TEST_LLM_MEM_MAX_G) ]; then max_g=$(TEST_LLM_MEM_MAX_G); fi; \
	if [ $$max_g -lt $(TEST_LLM_MEM_MIN_G) ]; then \
		echo "test-llm: $${avail_g}G free, needs $$(($(TEST_LLM_MEM_MIN_G) + $(TEST_LLM_HEADROOM_G)))G; free some memory and retry" >&2; \
		exit 1; \
	fi; \
	echo "test-llm: memory cap $${max_g}G ($${avail_g}G free)"; \
	trap 'status=$$?; $(CHECK_DB_STOP); exit $$status' EXIT; \
	$(CHECK_DB_START); \
	TEST_DATABASE_URL="$(CHECK_TEST_DATABASE_URL)" $(CHECK_CARGO_ENV) systemd-run --user --scope -q --slice=background.slice -p MemoryHigh=$$((max_g - 2))G -p MemoryMax=$${max_g}G cargo nextest run --build-jobs $(CHECK_BUILD_JOBS) --no-fail-fast --failure-output final $(ARGS)

# The Lateania battle arena: every balance contract and both report parts
# (late-ssh/target/lateania-arena*.md). Thousands of real engine fights, so
# it is not part of the suite; run it when combat, classes, gear, or bosses
# change. Serial (-j1): the report tests each need most of a CPU-minute budget.
.PHONY: arena
arena:
	$(MAKE) test-llm ARGS="-p late-ssh --lib --run-ignored all -j1 -E 'test(lateania::svc::arena)'"

# The deadchannel balance arena (late-ssh/src/app/deadchannel/fight/BALANCE.md):
# every contract plus the report, every table for the live rules, written to
# late-ssh/target/deadchannel-arena.md. Run it when the fight's numbers or
# rules change.
.PHONY: deadchannel-arena
deadchannel-arena:
	$(MAKE) test-llm ARGS="-p late-ssh --lib --run-ignored all --no-capture -E 'test(deadchannel::fight::arena) and not test(arena_sweep) and not test(arena_crystals)'"

# The balance sweep: one reading per candidate set of rules in
# arena_test.rs::SWEEP, side by side, written to
# late-ssh/target/deadchannel-sweep.md. Edit SWEEP, run this, read the table.
.PHONY: deadchannel-sweep
deadchannel-sweep:
	$(MAKE) test-llm ARGS="-p late-ssh --lib --run-ignored all --no-capture -E 'test(arena_sweep)'"

# The crystal question (fight/BALANCE.md, "What a crystal buys"): the crystal
# players side by side under the live rules and with pours priced in bits,
# written to late-ssh/target/deadchannel-crystals.md.
.PHONY: deadchannel-crystals
deadchannel-crystals:
	$(MAKE) test-llm ARGS="-p late-ssh --lib --run-ignored all --no-capture -E 'test(arena_crystals)'"

# Whole simulated runs, start to finish: one seeded climb a player from a
# fresh row to the first mark, every shop visit, step, and turn, written to
# late-ssh/target/deadchannel-run.md. Read it to see what a day of the
# rules is like.
.PHONY: deadchannel-run
deadchannel-run:
	$(MAKE) test-llm ARGS="-p late-ssh --lib --run-ignored all --no-capture -E 'test(arena_run)'"

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

# Drop the per-test databases that killed test runs leave behind. Each test
# makes its own and drops it on the way out, but a cancelled or timed-out run
# does not — and a few thousand of them make postgres crawl on startup, which
# shows up as `make start` failing on an unhealthy database.
.PHONY: test-db-clean
test-db-clean:
	@docker compose exec -T postgres psql -U $${LATE_DB_USER:-postgres} -t -A \
		-c "SELECT datname FROM pg_database WHERE datname LIKE 'test\_%'" \
		| awk '{print "DROP DATABASE IF EXISTS \""$$0"\";"}' \
		| docker compose exec -T postgres psql -U $${LATE_DB_USER:-postgres} -q -f -
	@echo "leftover test databases dropped (templates left alone)"

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
# `down` takes the containers away and keeps the database (the named
# `postgres-data` volume); `remove` wipes the database too. Not `down -v`:
# that also drops the cargo caches and forces a full rebuild.
down:
	docker compose -f docker-compose.yml -f docker-compose.monitoring.yml down
stop:
	docker compose -f docker-compose.yml -f docker-compose.monitoring.yml stop
remove: down
	docker volume rm -f "$$(docker compose -f docker-compose.yml config | sed -n 's/^name: //p')_postgres-data"

# Stops every running container on this machine, not just this project's.
.PHONY: stop-all
stop-all:
	docker ps -q | xargs -r docker stop

.PHONY: seed-calendar
seed-calendar: .env
	./scripts/seed_calendar_test_data.sh
