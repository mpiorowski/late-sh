#!/usr/bin/env bash
# late.sh only: reset the prod Vintage Story world with a seed and landcover / oceanscale.
# scripts/vintagestory_reroll.sh <seed|random> <landcover> <oceanscale> [backup-file]
# follows late-ssh/src/app/door/vintagestory/CONTEXT.md, Operations, "Resetting the world".
# needs kubectl pointed at the late.sh cluster.
set -euo pipefail
seed="$1"; landcover="$2"; oceanscale="$3"; backup="${4:-}"
dir="$(mktemp -d)"
trap 'rm -rf "$dir"' EXIT
helper=vs-reset-helper

kubectl scale deploy/vintagestory --replicas=0
kubectl wait --for=delete pod -l app=vintagestory --timeout=180s || true

kubectl delete pod "$helper" --ignore-not-found --wait
kubectl run "$helper" --image=busybox:1.36 --restart=Never --overrides='{
  "spec": {
    "nodeSelector": {"role": "games", "kubernetes.io/arch": "arm64"},
    "tolerations": [{"key": "role", "operator": "Equal", "value": "games", "effect": "NoSchedule"}],
    "containers": [{"name": "h", "image": "busybox:1.36", "command": ["sleep", "3600"],
      "volumeMounts": [{"name": "d", "mountPath": "/data"}]}],
    "volumes": [{"name": "d", "persistentVolumeClaim": {"claimName": "vintagestory-data"}}]
  }}'
kubectl wait --for=condition=Ready pod/"$helper" --timeout=120s

if [ -n "$backup" ]; then
  kubectl exec "$helper" -- tar czf - -C /data Saves > "$backup"
  ls -la "$backup"
fi

kubectl exec "$helper" -- cat /data/serverconfig.json > "$dir/serverconfig.json"
if [ "$seed" = random ]; then seedjson=null; else seedjson="$(jq -n --arg s "$seed" '$s')"; fi
jq --argjson seed "$seedjson" --arg lc "$landcover" --arg os "$oceanscale" '
  .WorldConfig.Seed = $seed
  | .WorldConfig.WorldConfiguration.landcover = $lc
  | .WorldConfig.WorldConfiguration.oceanscale = $os' \
  "$dir/serverconfig.json" > "$dir/serverconfig.new.json"
jq '.WorldConfig | {Seed, WorldConfiguration}' "$dir/serverconfig.new.json"
kubectl exec -i "$helper" -- sh -c 'cat > /data/serverconfig.json && chown 1654:1654 /data/serverconfig.json' < "$dir/serverconfig.new.json"
kubectl exec "$helper" -- sh -c 'rm -rf /data/Saves/* /data/Backups/* /data/BackupSaves/*; ls -la /data/Saves'

kubectl delete pod "$helper" --wait
kubectl scale deploy/vintagestory --replicas=1
for i in $(seq 1 60); do
  if kubectl logs deploy/vintagestory 2>/dev/null | grep -q "Dedicated Server now running"; then
    echo "server up"; exit 0
  fi
  sleep 5
done
echo "server not up after 5 min"; exit 1
