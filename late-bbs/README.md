# late-bbs

Local Docker proof stack for the standalone LORD BBS V1 described in
`DRAGON.md`.

V1 is deliberately not integrated with the late.sh app. Prove it locally with
Docker first, then optionally deploy the same image to the internal Kubernetes
stack later.

## Legal Boundary

Do not commit LORD archives, extracted LORD files, activation codes, generated
data files, game text, or screenshots from the registered package.

Buy/register the BBS version separately, then install it into the mounted data
directory at:

```text
/bbs/doors/lord
```

For local Docker, that path maps to:

```text
./tmp/lord-bbs/doors/lord
```

## Local Docker

From the repo root:

```bash
mkdir -p ./tmp/lord-bbs
docker compose -f docker-compose.lord-bbs.yml up --build
```

Connect locally:

```bash
telnet localhost 2323
```

For final ANSI/CP437 validation, use SyncTERM or another real BBS terminal
client against `localhost:2323`.

The compose file persists BBS config, users, door files, and LORD state under
`./tmp/lord-bbs`, which is intentionally gitignored.

## Install Registered LORD Locally

Put the registered LORD BBS archive somewhere outside git, then copy/extract it
into the local data directory:

```bash
mkdir -p ./tmp/lord-bbs/doors/lord
unzip /path/to/lord-registered.zip -d ./tmp/lord-bbs/doors/lord
```

Run the LORD setup program inside the container:

```bash
docker compose -f docker-compose.lord-bbs.yml exec lord-bbs bash
cd /bbs/doors/lord
dosemu SETUP.EXE
```

Use the exact setup executable and activation flow from the registered package.
Keep activation codes only in local/private storage.

## Synchronet Door Setup

Configure Synchronet locally with `scfg`:

```bash
docker compose -f docker-compose.lord-bbs.yml exec lord-bbs \
  gosu sbbs env SBBSCTRL=/bbs/sbbs/ctrl SBBSEXEC=/bbs/sbbs/exec /bbs/sbbs/exec/scfg
```

Add LORD as an external program. Use `/bbs/sbbs/exec/lord-runner` as the wrapper
and pass the exact command-line required by the registered LORD package after
local testing confirms it. The current wrapper intentionally fails with a clear
message when LORD files are absent.

## Optional Kubernetes Deploy

After the local Docker proof works, the same image can be deployed internally
with the optional Terraform stack in `infra/lord-bbs.tf`.

Build and push:

```bash
docker buildx build \
  --platform linux/amd64 \
  -t ghcr.io/<owner>/<repo>/lord-bbs:<tag> \
  -f late-bbs/Dockerfile \
  --push \
  late-bbs
```

Set these Terraform variables in the target environment:

```text
LORD_BBS_ENABLED=true
LORD_BBS_IMAGE_TAG=ghcr.io/<owner>/<repo>/lord-bbs:<tag>
```

The service is ClusterIP-only. For manual testing, use port-forwarding:

```bash
kubectl -n lord-bbs port-forward svc/lord-bbs-sv 2323:23
telnet localhost 2323
```

## V1 Verification Checklist

- Fresh caller can create/login to a BBS account.
- Caller can launch LORD from the external-program menu.
- Caller can spend forest turns, exit cleanly, reconnect, and keep state.
- Two simultaneous callers can enter without corrupting LORD data.
- ANSI and CP437 output render correctly in SyncTERM or another normal BBS
  terminal.
- Container restart preserves BBS users, LORD config, score/state files, and
  door setup through `./tmp/lord-bbs`.
