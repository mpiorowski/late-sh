# =============================================================================
# Door-game hosts: one module instance per game
# =============================================================================
# Every standalone door host (identity secret, save PVC, deployment, service)
# is stamped out of the door module (./door) from this map. Adding a game is
# one entry here plus the workflow lists (see release.yml).
#
# var_path values MUST match the path baked into each host image (see the
# runtime-<name> stage in the root Dockerfile and the crate's config.rs).
# Ports match the host's compiled-in SSH listener.

locals {
  doors = {
    # NetHack: the writable playground (per-player saves, shared bones, locks,
    # record) lives at the binary's compiled-in VAR_PLAYGROUND; read-only data
    # stays in the image at HACKDIR. NetHack never creates its own save/
    # subdirectory, so the seed command creates it (plus the append-only
    # record files) and sweeps orphaned ?lock.* getlock files left by hard
    # SIGKILLs. The sweep is safe only because the deploy strategy is
    # kill-before-create: no other pod can hold a live lock when it runs. It
    # does NOT touch save/*.gz. The ?lock.* glob matches any single-char slot
    # prefix on purpose, so it stays correct as MAXPLAYERS changes; without
    # it, leaked slots accumulate until the door wedges for everyone.
    nethack = {
      port                = 2323
      var_path            = "/var/games/nethack-var"
      pvc_size            = "2Gi"
      seed_container_name = "nethack-save-seed"
      seed_command        = "mkdir -p /var/games/nethack-var/save && touch /var/games/nethack-var/record /var/games/nethack-var/logfile /var/games/nethack-var/xlogfile /var/games/nethack-var/livelog /var/games/nethack-var/perm && chown -R late:late /var/games/nethack-var && rm -f /var/games/nethack-var/?lock.*"
      extra_env           = {}
      extra_ports         = []
    }

    # dopewars: no mid-game save upstream; what persists is the single shared
    # high-score file every session's `-f` points at. The binary creates the
    # .sco itself on first score write and locks it during updates.
    dopewars = {
      port                = 2324
      var_path            = "/var/lib/late-dopewars"
      pvc_size            = "256Mi"
      seed_container_name = "dopewars-score-seed"
      seed_command        = "mkdir -p /var/lib/late-dopewars && chown -R late:late /var/lib/late-dopewars"
      extra_env = {
        LATE_DOPEWARS_SCORE_FILE = "/var/lib/late-dopewars/dopewars.sco"
      }
      extra_ports = []
    }

    # DCSS: the PVC is the child HOME; crawl creates its own ~/.crawl tree.
    # Port 2329 is the read-only crawl-file publisher for the public DCSS
    # tooling (late-dcss/src/publish.rs), fronted by the ingress below.
    dcss = {
      port                = 2325
      var_path            = "/var/lib/late-dcss"
      pvc_size            = "2Gi"
      seed_container_name = "dcss-save-seed"
      seed_command        = "mkdir -p /var/lib/late-dcss && chown -R late:late /var/lib/late-dcss"
      extra_env = {
        LATE_DCSS_DATA_DIR     = "/var/lib/late-dcss"
        LATE_DCSS_PUBLISH_PORT = "2329"
      }
      extra_ports = [
        { name = "crawl", port = 2329 }
      ]
    }

    # Usurper: one shared world (players, gangs, king, news) in the writable
    # game tree. The host copies missing seed files from the image at boot and
    # sweeps stale lock artifacts itself.
    usurper = {
      port                = 2326
      var_path            = "/var/lib/late-usurper"
      pvc_size            = "1Gi"
      seed_container_name = "usurper-save-seed"
      seed_command        = "mkdir -p /var/lib/late-usurper && chown -R late:late /var/lib/late-usurper"
      extra_env = {
        LATE_USURPER_GAME_DIR = "/var/lib/late-usurper"
      }
      extra_ports = []
    }

    # Brogue: per-player save directories under players/; the host creates
    # each on demand (our build carries the hangup-save patch, see
    # scripts/brogue_hangup_save.patch).
    brogue = {
      port                = 2327
      var_path            = "/var/lib/late-brogue"
      pvc_size            = "2Gi"
      seed_container_name = "brogue-save-seed"
      seed_command        = "mkdir -p /var/lib/late-brogue && chown -R late:late /var/lib/late-brogue"
      extra_env = {
        LATE_BROGUE_DATA_DIR = "/var/lib/late-brogue"
      }
      extra_ports = []
    }

    # CodeKeep: one autosaved campaign beneath each account-specific HOME.
    # The host's one-live-child-per-account lease is process-local, another
    # reason replicas stay 1.
    codekeep = {
      port                = 2328
      var_path            = "/var/lib/late-codekeep"
      pvc_size            = "1Gi"
      seed_container_name = "codekeep-save-seed"
      seed_command        = "mkdir -p /var/lib/late-codekeep && chown -R late:late /var/lib/late-codekeep"
      extra_env = {
        LATE_CODEKEEP_DATA_DIR = "/var/lib/late-codekeep"
      }
      extra_ports = []
    }

    # BashQuest: bashquest.sh keeps everything under $HOME/.bashquest, one
    # shared directory (not per-player) so its in-game leaderboard sees every
    # player's save. It creates users.db and the save files itself and saves
    # continuously (save_progress() call sites), so there is no hangup-save
    # dance.
    bashquest = {
      port                = 2330
      var_path            = "/var/lib/late-bashquest"
      pvc_size            = "256Mi"
      seed_container_name = "bashquest-save-seed"
      seed_command        = "mkdir -p /var/lib/late-bashquest && chown -R late:late /var/lib/late-bashquest"
      extra_env = {
        LATE_BASHQUEST_DATA_DIR = "/var/lib/late-bashquest"
      }
      extra_ports = []
    }
  }
}

module "door" {
  source   = "./door"
  for_each = local.doors

  name                = each.key
  port                = each.value.port
  image               = local.image_tags[each.key]
  var_path            = each.value.var_path
  pvc_size            = each.value.pvc_size
  seed_container_name = each.value.seed_container_name
  seed_command        = each.value.seed_command
  extra_env           = each.value.extra_env
  extra_ports         = each.value.extra_ports
  log_level           = var.LOG_LEVEL
  pull_secret_name    = kubernetes_secret_v1.regcred.metadata[0].name
  cpu_request         = local.door_cpu_request
  memory_request      = local.door_memory_request
  cpu_limit           = local.door_cpu_limit
  memory_limit        = local.door_memory_limit

  depends_on = [
    helm_release.local_path_provisioner
  ]
}

# =============================================================================
# late.sh/crawl → the read-only crawl server files (logfile, milestones, morgue)
# =============================================================================
# Public DCSS tooling (dcss-stats.com, Sequell) ingests a server by fetching its
# logfile/milestones over HTTP and linking morgue dumps per game. The late-dcss
# host serves those natively under /crawl/... (late-dcss/src/publish.rs), so no
# rewrite annotation is needed here.
#
# Path prefix rather than a subdomain, deliberately: ingress-nginx merges every
# rule for a host into one server block and matches longest-prefix-first, so
# /crawl out-ranks late-web's "/" catch-all in ingress.tf while the existing DNS
# record and TLS certificate keep working untouched. That is also why this
# resource carries no tls block and no cert-manager annotations: the certificate
# for local.domain belongs to service-web-ingress, and a second issuer request
# for the same host would fight it.
resource "kubernetes_ingress_v1" "late_dcss_crawl" {
  metadata {
    name = "late-dcss-crawl-ingress"
    annotations = {
      "kubernetes.io/ingress.class" = "nginx"
    }
  }

  spec {
    rule {
      host = local.domain
      http {
        path {
          path      = "/crawl"
          path_type = "Prefix"
          backend {
            service {
              name = module.door["dcss"].service_name
              port {
                name = "crawl"
              }
            }
          }
        }
      }
    }
  }
}

# =============================================================================
# State moves: pre-module flat resources -> module instances
# =============================================================================
# The doors used to be one hand-written file pair per game; these map every
# existing resource to its module address so the refactor is a pure rename.
# The first plan after this lands must show moves only, never any
# create/destroy of door resources (the PVCs hold player saves).

moved {
  from = kubernetes_deployment_v1.late_nethack
  to   = module.door["nethack"].kubernetes_deployment_v1.this
}

moved {
  from = kubernetes_service_v1.late_nethack_sv
  to   = module.door["nethack"].kubernetes_service_v1.this
}

moved {
  from = kubernetes_persistent_volume_claim_v1.nethack_save
  to   = module.door["nethack"].kubernetes_persistent_volume_claim_v1.save
}

moved {
  from = random_password.nethack_identity_secret
  to   = module.door["nethack"].random_password.identity
}

moved {
  from = kubernetes_secret_v1.nethack_identity_secret
  to   = module.door["nethack"].kubernetes_secret_v1.identity
}

moved {
  from = kubernetes_deployment_v1.late_dopewars
  to   = module.door["dopewars"].kubernetes_deployment_v1.this
}

moved {
  from = kubernetes_service_v1.late_dopewars_sv
  to   = module.door["dopewars"].kubernetes_service_v1.this
}

moved {
  from = kubernetes_persistent_volume_claim_v1.dopewars_save
  to   = module.door["dopewars"].kubernetes_persistent_volume_claim_v1.save
}

moved {
  from = random_password.dopewars_identity_secret
  to   = module.door["dopewars"].random_password.identity
}

moved {
  from = kubernetes_secret_v1.dopewars_identity_secret
  to   = module.door["dopewars"].kubernetes_secret_v1.identity
}

moved {
  from = kubernetes_deployment_v1.late_dcss
  to   = module.door["dcss"].kubernetes_deployment_v1.this
}

moved {
  from = kubernetes_service_v1.late_dcss_sv
  to   = module.door["dcss"].kubernetes_service_v1.this
}

moved {
  from = kubernetes_persistent_volume_claim_v1.dcss_save
  to   = module.door["dcss"].kubernetes_persistent_volume_claim_v1.save
}

moved {
  from = random_password.dcss_identity_secret
  to   = module.door["dcss"].random_password.identity
}

moved {
  from = kubernetes_secret_v1.dcss_identity_secret
  to   = module.door["dcss"].kubernetes_secret_v1.identity
}

moved {
  from = kubernetes_deployment_v1.late_usurper
  to   = module.door["usurper"].kubernetes_deployment_v1.this
}

moved {
  from = kubernetes_service_v1.late_usurper_sv
  to   = module.door["usurper"].kubernetes_service_v1.this
}

moved {
  from = kubernetes_persistent_volume_claim_v1.usurper_save
  to   = module.door["usurper"].kubernetes_persistent_volume_claim_v1.save
}

moved {
  from = random_password.usurper_identity_secret
  to   = module.door["usurper"].random_password.identity
}

moved {
  from = kubernetes_secret_v1.usurper_identity_secret
  to   = module.door["usurper"].kubernetes_secret_v1.identity
}

moved {
  from = kubernetes_deployment_v1.late_brogue
  to   = module.door["brogue"].kubernetes_deployment_v1.this
}

moved {
  from = kubernetes_service_v1.late_brogue_sv
  to   = module.door["brogue"].kubernetes_service_v1.this
}

moved {
  from = kubernetes_persistent_volume_claim_v1.brogue_save
  to   = module.door["brogue"].kubernetes_persistent_volume_claim_v1.save
}

moved {
  from = random_password.brogue_identity_secret
  to   = module.door["brogue"].random_password.identity
}

moved {
  from = kubernetes_secret_v1.brogue_identity_secret
  to   = module.door["brogue"].kubernetes_secret_v1.identity
}

moved {
  from = kubernetes_deployment_v1.late_codekeep
  to   = module.door["codekeep"].kubernetes_deployment_v1.this
}

moved {
  from = kubernetes_service_v1.late_codekeep_sv
  to   = module.door["codekeep"].kubernetes_service_v1.this
}

moved {
  from = kubernetes_persistent_volume_claim_v1.codekeep_save
  to   = module.door["codekeep"].kubernetes_persistent_volume_claim_v1.save
}

moved {
  from = random_password.codekeep_identity_secret
  to   = module.door["codekeep"].random_password.identity
}

moved {
  from = kubernetes_secret_v1.codekeep_identity_secret
  to   = module.door["codekeep"].kubernetes_secret_v1.identity
}

moved {
  from = kubernetes_deployment_v1.late_bashquest
  to   = module.door["bashquest"].kubernetes_deployment_v1.this
}

moved {
  from = kubernetes_service_v1.late_bashquest_sv
  to   = module.door["bashquest"].kubernetes_service_v1.this
}

moved {
  from = kubernetes_persistent_volume_claim_v1.bashquest_save
  to   = module.door["bashquest"].kubernetes_persistent_volume_claim_v1.save
}

moved {
  from = random_password.bashquest_identity_secret
  to   = module.door["bashquest"].random_password.identity
}

moved {
  from = kubernetes_secret_v1.bashquest_identity_secret
  to   = module.door["bashquest"].kubernetes_secret_v1.identity
}
