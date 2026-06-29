# agentbbs-gcp

GCP-backed **sysops/admin reporting** for AgentBBS. It implements
`agentbbs-core`'s `Reporter` trait two ways — a **Firestore document sink** and
a **Pub/Sub publisher** — and is built to run entirely against the **local
Firestore and Pub/Sub emulators**, so development works offline.

```
AgentBBS ──report()──► PubSubReporter ──REST──► [Pub/Sub topic: agentbbs-events]
                                                       │
                                                       ▼
                                                Cloud Function (functions/)
                                                       │  aggregate()
                                                       ▼
                                        [Firestore] sysop_reports/latest

AgentBBS ──report()──► FirestoreReporter ──REST──► [Firestore] agentbbs_events
```

## Crate layout

| Module          | Purpose                                                                 |
|-----------------|-------------------------------------------------------------------------|
| `encode`        | Pure `Event → REST JSON`: `to_firestore_fields`, `pubsub_publish_body`. |
| `env`           | Emulator-aware base-URL selection from the `*_EMULATOR_HOST` env vars.   |
| `firestore`     | `FirestoreReporter` — writes each event as a document.                  |
| `pubsub`        | `PubSubPublisher` + `PubSubReporter`.                                   |
| `aggregate`     | Pure `aggregate() -> SysopReport`; canonical logic the function mirrors. |

### The sync-report / async-HTTP bridge

`core::Reporter::report` is **synchronous and non-blocking** — it is called from
hot paths and must never block on the network or fail the caller. Firestore and
Pub/Sub are async HTTP. Both reporters bridge this with an **unbounded tokio
mpsc channel**: `report()` does a lock-free `send` and returns immediately,
while a background task spawned on a provided runtime `Handle` drains the
receiver and performs the REST calls. Transport errors are logged, never fatal.

```rust
use tokio::runtime::Handle;
use agentbbs_core::report::{Event, EventKind, Reporter};
use agentbbs_gcp::{FirestoreReporter, PubSubReporter, DEFAULT_TOPIC};

let fs = FirestoreReporter::start("demo-project", None, &Handle::current());
let ps = PubSubReporter::start("demo-project", DEFAULT_TOPIC, None, &Handle::current());

fs.report(Event::now(EventKind::SessionOpen, "front-door")).ok();
ps.report(Event::now(EventKind::Post, "general")).ok();
```

Passing `None` for the base URL derives it from the emulator env var (falling
back to the production endpoint); pass `Some("http://host:port")` to override.

## Running the emulators locally

Set the env vars and the reporters automatically target the emulators.

### Option A — gcloud emulators

```bash
# One-time: gcloud components install beta cloud-firestore-emulator pubsub-emulator
gcloud beta emulators firestore start --host-port=localhost:8080 &
gcloud beta emulators pubsub start --host-port=localhost:8085 --project=demo-project &

export FIRESTORE_EMULATOR_HOST=localhost:8080
export PUBSUB_EMULATOR_HOST=localhost:8085
```

### Option B — docker images

```bash
docker run --rm -p 8080:8080 \
  gcr.io/google.com/cloudsdktool/google-cloud-cli:latest \
  gcloud beta emulators firestore start --host-port=0.0.0.0:8080

docker run --rm -p 8085:8085 \
  gcr.io/google.com/cloudsdktool/google-cloud-cli:latest \
  gcloud beta emulators pubsub start --host-port=0.0.0.0:8085 --project=demo-project

export FIRESTORE_EMULATOR_HOST=localhost:8080
export PUBSUB_EMULATOR_HOST=localhost:8085
```

### Option C — docker compose (recommended)

A ready-made compose file brings up both emulators on the standard ports and
creates the `agentbbs-events` topic automatically:

```bash
docker compose -f docker-compose.emulators.yml up

# In another shell, point the reporters at the emulators:
export FIRESTORE_EMULATOR_HOST=localhost:8080
export PUBSUB_EMULATOR_HOST=localhost:8085

# Run the emulator smoke tests (mold may be absent -> use lld):
RUSTFLAGS="-Clink-arg=-fuse-ld=lld" cargo test -p agentbbs-gcp -- --ignored

docker compose -f docker-compose.emulators.yml down
```

The `create-topic` one-shot service in the compose file PUTs the
`agentbbs-events` topic into the Pub/Sub emulator (project `demo-project`) once
it is healthy, so you don't need the manual `curl` below.

### Create the Pub/Sub topic in the emulator

The emulator starts empty; create the topic before publishing:

```bash
curl -s -X PUT \
  "http://${PUBSUB_EMULATOR_HOST}/v1/projects/demo-project/topics/agentbbs-events"
```

### Required environment variables

| Variable                  | Example           | Effect                                            |
|---------------------------|-------------------|---------------------------------------------------|
| `FIRESTORE_EMULATOR_HOST` | `localhost:8080`  | Firestore REST → `http://localhost:8080`.         |
| `PUBSUB_EMULATOR_HOST`    | `localhost:8085`  | Pub/Sub REST → `http://localhost:8085`.           |

When a variable is unset the corresponding client falls back to the real Google
endpoint (`https://firestore.googleapis.com` / `https://pubsub.googleapis.com`).

## The reporter → topic → function → sysop_reports flow

1. AgentBBS calls `report(event)` on a `PubSubReporter`.
2. The reporter base64-JSON-encodes the event and POSTs it to the
   `agentbbs-events` topic's `:publish` endpoint.
3. The 2nd-gen Cloud Function in [`functions/`](./functions) is triggered by the
   topic. It base64-decodes the message, parses the core `Event`, and folds it
   into the running `sysop_reports/latest` Firestore document.
4. The function's fold logic mirrors the canonical Rust `aggregate()` in
   [`src/aggregate.rs`](./src/aggregate.rs): `total`, `by_kind`, `warnings`,
   `criticals`, and a tail of recent `{kind, subject}` summaries.

`FirestoreReporter` is the simpler, direct path: it writes each event straight
into the `agentbbs_events` collection (no function in the loop).

## Tests

All Rust tests run offline (no emulator). Tests that hit a live emulator are
marked `#[ignore]`.

```bash
# NOTE: the repo's .cargo/config.toml forces the `mold` linker, which may not be
# installed. Use lld:
RUSTFLAGS="-Clink-arg=-fuse-ld=lld" cargo test -p agentbbs-gcp
```

Covered without network: exact `to_firestore_fields` shape, `pubsub_publish_body`
base64 shape, `aggregate` totals/by_kind/warnings/criticals, and env-var base-URL
selection. To run the emulator smoke tests, start the emulators, export the env
vars, create the topic, and run `cargo test -p agentbbs-gcp -- --ignored`.

## Cloud Function (`functions/`)

TypeScript, 2nd-gen, Pub/Sub-triggered.

```bash
cd functions
npm install        # NOT run by this repo's tooling
npm run build      # tsc → dist/
npm start          # local functions-framework, cloudevent signature
```

## Deploying with Terraform (`terraform/`)

Reviewable config (do not blind-apply). It creates the Firestore database
(native mode), the `agentbbs-events` topic + subscription, and the 2nd-gen
Cloud Function triggered by the topic.

```bash
cd terraform

# Zip and upload the function source first, then point the vars at it.
( cd ../functions && npm install && npm run build \
  && zip -r /tmp/sysop-report-function.zip dist package.json node_modules )
gsutil cp /tmp/sysop-report-function.zip \
  gs://<your-bucket>/agentbbs/sysop-report-function.zip

terraform init
terraform plan \
  -var project_id=<your-project> \
  -var function_source_bucket=<your-bucket>
# Review the plan, then:
terraform apply -var project_id=<your-project> -var function_source_bucket=<your-bucket>
```

Outputs include the topic id/name, subscription id, Firestore database name, and
the function name/URI.
