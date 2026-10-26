#!/usr/bin/env bash
# Runs late-ssh locally the way prod runs it: pods behind an edge, rolled out
# the way the prod Deployment rolls (infra/service-ssh.tf: a new pod surges
# in, and once it is ready an old one gets SIGTERM and drains). The edge and
# the pod template are services in docker-compose.yml; this script owns the
# pods. Each pod runs its generation's binary from target/fleet/g<gen> in the
# cargo-target volume, so a draining pod keeps the code it started with.
# Host-side it sticks to bash 3.2, the one macOS ships.
set -euo pipefail
ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$ROOT_DIR"
SELF="$ROOT_DIR/scripts/fleet.sh"

usage() {
  cat <<'EOF'
scripts/fleet.sh start <replicas> <plain|monitoring>
    bring the stack up on the working tree, then roll out every change to it;
    Ctrl-C stops the stack (make start, make startm)
scripts/fleet.sh rollout <replicas> <always|if-changed>
    build the working tree as the next generation and roll to it; if-changed
    stops when the binary matches the newest ready pod's (make fleet-rollout)
scripts/fleet.sh up <replicas>
    build a first generation and start the stack around it (start runs it)
scripts/fleet.sh ps        every pod with its generation and state (make fleet-ps)
scripts/fleet.sh logs      follow every running pod's log (make fleet-logs)
scripts/fleet.sh expire    kill the draining pods, as the 6 h grace deadline does (make fleet-expire)
scripts/fleet.sh stop      stop the pods and the stack (make stop)
scripts/fleet.sh down      remove the pods and the stack; the database stays (make down)
EOF
}

BASE_FILES=(-f docker-compose.yml)
ALL_FILES=(-f docker-compose.yml -f docker-compose.monitoring.yml)
# Everything a change can reach the late-ssh binary through. A change rolls
# out only when it does: the watcher builds first and compares binaries.
WATCHED=(Cargo.toml Cargo.lock late-ssh late-core assets)
# A new pod gets this long to answer /api/health with 200, migrations included.
READY_TIMEOUT_SECS=600
# The edge resolves `ssh-pod` and checks each pod once a second; a ready pod
# gets this long to enter rotation before an old one leaves it.
EDGE_PICKUP_SECS=3

required_env() {
  local value
  value=$(grep -E "^$1=" .env | tail -n 1 | cut -d= -f2- || true)
  if [[ -z $value ]]; then echo ".env has no $1; run this through make" >&2; exit 2; fi
  echo "$value"
}
INSTANCE=$(required_env INSTANCE)
API_PORT=$(required_env LATE_API_PORT)
PROJECT=$(docker compose "${BASE_FILES[@]}" config | sed -n 's/^name: //p')
ROLLOUT_LOCK="${TMPDIR:-/tmp}/late-fleet-$PROJECT.rollout"
START_LOCK="${TMPDIR:-/tmp}/late-fleet-$PROJECT.start"

monitoring_running() {
  [[ -n $(docker ps -q \
    --filter "label=com.docker.compose.project=$PROJECT" \
    --filter label=com.docker.compose.service=otel-collector) ]]
}

# Compose over the files the running stack uses: the monitoring overlay joins
# whenever its collector runs, so pods under `make startm` export telemetry.
stack_compose() {
  if monitoring_running; then
    docker compose "${ALL_FILES[@]}" "$@"
  else
    docker compose "${BASE_FILES[@]}" "$@"
  fi
}

# Every pod of this project, oldest generation first, one per line:
# "<gen> <name> <docker state>".
pods() {
  docker ps -a \
    --filter "label=com.docker.compose.project=$PROJECT" \
    --filter label=late.fleet.gen \
    --format '{{.Label "late.fleet.gen"}} {{.Names}} {{.State}}' |
    sort -k1,1n -k2,2
}

has_running_pods() {
  [[ -n $(pods | awk '$3 == "running"') ]]
}

# What a running pod's /api/health answers: ready (200), draining (the 503 a
# draining pod gives), or not-ready (anything else, no answer included).
probe() {
  local response
  response=$(docker exec "$1" timeout 2 bash -c \
    "exec 3<>/dev/tcp/localhost/$API_PORT; printf 'GET /api/health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n' >&3; cat <&3" \
    2>/dev/null) || true
  case $response in
    'HTTP/1.1 200'*) echo ready ;;
    *draining*) echo draining ;;
    *) echo not-ready ;;
  esac
}

newest_serving_generation() {
  local gen name state newest=""
  while read -r gen name state; do
    if [[ $state == running && $(probe "$name") == ready ]]; then newest=$gen; fi
  done < <(pods)
  echo "$newest"
}

# The oldest pod that still serves new connections from a generation before
# <gen>, if any.
oldest_serving_before() {
  local before=$1 gen name state
  while read -r gen name state; do
    if ((gen < before)) && [[ $state == running && $(probe "$name") == ready ]]; then
      echo "$name"
      return
    fi
  done < <(pods)
}

# Exited pods are spent, like a terminated pod k8s cleans up; keeping them
# only holds their generation's binary.
remove_exited_pods() {
  local gen name state
  while read -r gen name state; do
    if [[ $state != running ]]; then docker rm "$name" >/dev/null; fi
  done < <(pods)
}

next_generation() {
  local last
  last=$(pods | tail -n 1 | cut -d' ' -f1)
  echo $((${last:-0} + 1))
}

# A lock is a directory holding its owner's pid: mkdir is atomic everywhere,
# and an owner that died leaves a pid nothing answers to.
claim() {
  local dir=$1 holder
  if mkdir "$dir" 2>/dev/null; then
    echo $$ >"$dir/pid"
    return 0
  fi
  holder=$(cat "$dir/pid" 2>/dev/null || true)
  if [[ -z $holder ]] || kill -0 "$holder" 2>/dev/null; then return 1; fi
  rm -rf "$dir"
  claim "$dir"
}

# Build the working tree in the shared cargo-target volume and keep it as
# target/fleet/g<gen>, dropping the binaries no pod runs any more. With
# if-changed it returns 3 instead when the build matches generation <current>.
build_generation() {
  local gen=$1 mode=$2 current=$3 keep
  keep=$(pods | cut -d' ' -f1 | sort -u | tr '\n' ' ')
  echo "building generation $gen from the working tree"
  stack_compose run --rm --no-deps -T \
    -e "FLEET_GEN=$gen" -e "FLEET_MODE=$mode" -e "FLEET_CURRENT=$current" -e "FLEET_KEEP=$keep" \
    ssh-pod bash -c '
      set -euo pipefail
      cargo build --features otel -p late-ssh
      current="target/fleet/g$FLEET_CURRENT/late-ssh"
      if [[ $FLEET_MODE == if-changed && -n $FLEET_CURRENT && -f $current ]] &&
        cmp -s target/debug/late-ssh "$current"; then
        exit 3
      fi
      install -D target/debug/late-ssh "target/fleet/g$FLEET_GEN/late-ssh"
      for dir in target/fleet/g*; do
        case " $FLEET_KEEP $FLEET_GEN " in
          *" ${dir#target/fleet/g} "*) ;;
          *) rm -rf "$dir" ;;
        esac
      done'
}

# Start one pod of a generation and print its name. with_deps starts postgres
# and the doors first; a running stack passes no_deps.
start_pod() {
  local gen=$1 index=$2 deps=$3 name
  name="$INSTANCE-ssh-g$gen-$index"
  local args=(run -d --use-aliases --name "$name" --label "late.fleet.gen=$gen"
    -e "OTEL_RESOURCE_ATTRIBUTES=service.instance.id=$name")
  case $deps in
    with_deps) ;;
    no_deps) args+=(--no-deps) ;;
  esac
  stack_compose "${args[@]}" ssh-pod "/app/target/fleet/g$gen/late-ssh" >/dev/null
  echo "$name"
}

wait_ready() {
  local pod=$1 waited=0
  echo "waiting for $pod to be ready"
  while ((waited < READY_TIMEOUT_SECS)); do
    if [[ $(docker inspect -f '{{.State.Running}}' "$pod") != true ]]; then return 1; fi
    if [[ $(probe "$pod") == ready ]]; then return 0; fi
    sleep 1
    waited=$((waited + 1))
  done
  return 1
}

# A pod that never got ready stops the rollout where it is, the pods it did
# not replace still serving, as a stalled Deployment does.
fail_pod() {
  local pod=$1
  echo "$pod never became ready; its last log lines:" >&2
  docker logs --tail 40 "$pod" >&2 || true
  docker rm -f "$pod" >/dev/null
  echo 'stopped; the pods that were serving still are (make fleet-ps)' >&2
  exit 1
}

# SIGTERM, what the kubelet sends: the pod drains and exits on its own once
# its last session ends.
drain() {
  docker kill --signal TERM "$1" >/dev/null
  echo "draining $1"
}

up() {
  local replicas=$1 gen index pod deps
  if has_running_pods; then
    echo 'pods are already running: `make fleet-rollout` rolls them' >&2
    exit 1
  fi
  if ! claim "$ROLLOUT_LOCK"; then echo 'another rollout is running' >&2; exit 75; fi
  trap 'rm -rf "$ROLLOUT_LOCK"' EXIT
  remove_exited_pods
  gen=$(next_generation)
  build_generation "$gen" always ""
  for index in $(seq 1 "$replicas"); do
    deps=no_deps
    if ((index == 1)); then deps=with_deps; fi
    pod=$(start_pod "$gen" "$index" "$deps")
    wait_ready "$pod" || fail_pod "$pod"
  done
  stack_compose up -d
  ps_pods
}

rollout() {
  local replicas=$1 mode=$2 gen current index pod status=0
  if ! has_running_pods; then
    echo 'the stack is not up: `make start` brings it up' >&2
    exit 1
  fi
  if ! claim "$ROLLOUT_LOCK"; then echo 'another rollout is running' >&2; exit 75; fi
  trap 'rm -rf "$ROLLOUT_LOCK"' EXIT
  remove_exited_pods
  gen=$(next_generation)
  current=$(newest_serving_generation)
  build_generation "$gen" "$mode" "$current" || status=$?
  case $status in
    0) ;;
    3)
      echo "the build matches generation $current; nothing to roll out"
      return 0
      ;;
    *)
      echo 'the build failed; the pods that were serving still are' >&2
      exit "$status"
      ;;
  esac
  for index in $(seq 1 "$replicas"); do
    pod=$(start_pod "$gen" "$index" no_deps)
    wait_ready "$pod" || fail_pod "$pod"
    sleep "$EDGE_PICKUP_SECS"
    pod=$(oldest_serving_before "$gen")
    if [[ -n $pod ]]; then drain "$pod"; fi
  done
  pod=$(oldest_serving_before "$gen")
  while [[ -n $pod ]]; do
    drain "$pod"
    pod=$(oldest_serving_before "$gen")
  done
  ps_pods
}

ps_pods() {
  local gen name state found=0
  while read -r gen name state; do
    if ((found == 0)); then printf '%-24s %-4s %s\n' POD GEN STATE; fi
    found=1
    if [[ $state == running ]]; then state=$(probe "$name"); fi
    printf '%-24s %-4s %s\n' "$name" "$gen" "$state"
  done < <(pods)
  if ((found == 0)); then echo 'no pods'; fi
}

# Log followers run in the background, where a script's jobs ignore Ctrl-C,
# so whoever starts them kills them on exit.
FOLLOWERS=""
FOLLOWED=" "

prefix() {
  local line
  while IFS= read -r line; do printf '%s | %s\n' "$1" "$line"; done
}

# Follow the running pods not followed yet; the rest of the arguments go to
# `docker logs`. Keyed by container id: once every pod is gone, generations
# count from 1 again and a name comes back.
follow_new_pods() {
  local id name
  while read -r id name; do
    case $FOLLOWED in *" $id "*) continue ;; esac
    docker logs -f "$@" "$id" > >(prefix "$name") 2>&1 &
    FOLLOWERS="$FOLLOWERS $!"
    FOLLOWED="$FOLLOWED$id "
  done < <(docker ps \
    --filter "label=com.docker.compose.project=$PROJECT" \
    --filter label=late.fleet.gen \
    --format '{{.ID}} {{.Names}}')
}

stop_followers() {
  # The pids are words on purpose.
  # shellcheck disable=SC2086
  if [[ -n $FOLLOWERS ]]; then kill $FOLLOWERS 2>/dev/null || true; fi
}

logs() {
  follow_new_pods --tail 20
  if [[ -z $FOLLOWERS ]]; then echo 'no running pods' >&2; exit 1; fi
  trap stop_followers EXIT
  trap 'exit 130' INT TERM
  wait
}

expire() {
  local gen name state expired=0
  while read -r gen name state; do
    if [[ $state != running || $(probe "$name") != draining ]]; then continue; fi
    docker kill "$name" >/dev/null
    echo "expired $name"
    expired=1
  done < <(pods)
  if ((expired == 0)); then echo 'no draining pods'; fi
}

stop_stack() {
  local gen name state running=""
  while read -r gen name state; do
    if [[ $state == running ]]; then running="$running $name"; fi
  done < <(pods)
  # shellcheck disable=SC2086
  if [[ -n $running ]]; then docker stop $running >/dev/null; fi
  docker compose "${ALL_FILES[@]}" stop
}

down() {
  local gen name state
  while read -r gen name state; do
    docker rm -f "$name" >/dev/null
  done < <(pods)
  docker compose "${ALL_FILES[@]}" down
}

# One deploy of the working tree: a rollout over the running pods, or a first
# generation when none run (a fresh start, or a first build that never came
# up). Each runs as its own process, so its errors stop it and not the watcher.
deploy() {
  local replicas=$1 mode=$2
  if has_running_pods; then
    "$SELF" rollout "$replicas" "$mode"
  else
    "$SELF" up "$replicas"
  fi
}

STAMP=""
STACK_FOLLOWED=0

on_start_exit() {
  stop_followers
  rm -f "$STAMP" "$STAMP.next"
  echo 'stopping the stack'
  stop_stack
  rm -rf "$START_LOCK"
}

start() {
  local replicas=$1 stack=$2 since status
  if ! claim "$START_LOCK"; then
    echo "make start is already running (pid $(cat "$START_LOCK/pid" 2>/dev/null)); \`make fleet-rollout\` rolls from here" >&2
    exit 1
  fi
  trap on_start_exit EXIT
  trap 'exit 130' INT TERM
  case $stack in
    monitoring) docker compose "${ALL_FILES[@]}" up -d otel-collector ;;
    plain) ;;
  esac
  stack_compose --profile fleet-pod build
  since=$(date +%s)
  STAMP=$(mktemp "${TMPDIR:-/tmp}/late-fleet-watch.XXXXXX")
  status=0
  if has_running_pods; then
    # Pods left running by an earlier start: roll them to the working tree and
    # bring the rest of the stack onto the images just built.
    "$SELF" rollout "$replicas" always || status=$?
    if ((status == 0)); then stack_compose up -d; fi
  else
    "$SELF" up "$replicas" || status=$?
  fi
  echo "watching ${WATCHED[*]}: a change that rebuilds the binary rolls out a new generation"
  while true; do
    if ((STACK_FOLLOWED == 0)) && has_running_pods; then
      stack_compose logs -f --since "$since" &
      FOLLOWERS="$FOLLOWERS $!"
      STACK_FOLLOWED=1
    fi
    follow_new_pods --since "$since"
    sleep 1
    if [[ -z $(find "${WATCHED[@]}" -newer "$STAMP" -type f -print -quit 2>/dev/null) ]]; then continue; fi
    # Let an editor finish writing, then mark what this build covers: an edit
    # made while it runs triggers the next one.
    sleep 0.5
    touch "$STAMP.next"
    status=0
    deploy "$replicas" if-changed || status=$?
    case $status in
      75) ;;
      0) mv "$STAMP.next" "$STAMP" ;;
      *)
        mv "$STAMP.next" "$STAMP"
        echo 'the next save tries again'
        ;;
    esac
  done
}

require_replicas() {
  if [[ ! ${1:-} =~ ^[1-9][0-9]*$ ]]; then usage >&2; exit 2; fi
}

case ${1:-} in
  start)
    require_replicas "${2:-}"
    case ${3:-} in plain | monitoring) ;; *) usage >&2; exit 2 ;; esac
    start "$2" "$3"
    ;;
  rollout)
    require_replicas "${2:-}"
    case ${3:-} in always | if-changed) ;; *) usage >&2; exit 2 ;; esac
    rollout "$2" "$3"
    ;;
  up) require_replicas "${2:-}"; up "$2" ;;
  ps) ps_pods ;;
  logs) logs ;;
  expire) expire ;;
  stop) stop_stack ;;
  down) down ;;
  -h | --help) usage ;;
  *) usage >&2; exit 2 ;;
esac
