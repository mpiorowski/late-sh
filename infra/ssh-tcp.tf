# =============================================================================
# SSH and IRC edge: host-network HAProxy on public :22 and :6697
# =============================================================================
# Owns the public SSH and IRC ports on IPv4 and IPv6 and forwards raw TCP to
# service-ssh-sv, prepending PROXY v1 so late-ssh and its ircd resolve real
# client IPs. HAProxy reaches the pod from the node address, which the prod
# profile trusts (late-ssh/src/config.rs). This enables: ssh late.sh and
# irc.late.sh.
#
# Deliberately not on ingress-nginx or ipv6-proxy: Ingress edits,
# cert-manager HTTP-01 solvers and controller rollouts reload or restart
# those, and nginx kills whatever its old workers hold 240 s after every
# reload (SCALE.md, Pain Point 9). This config only changes when SSH/IRC
# routing changes, and even then a rollout drains instead of cutting: the new
# pod surges in next to the old one (HAProxy binds with SO_REUSEPORT, so both
# hold the ports), then the old one soft-stops and keeps its open sessions.
#
# maxconn: late-ssh admits at most 1000 SSH and 200 IRC sessions and closes
# refused sockets on the spot, so 4096 is headroom that also keeps a flood
# inside the memory limit below.
# =============================================================================

locals {
  ssh_edge_haproxy_config = <<-EOT
    global
      log stdout format raw local0
      maxconn 4096

    defaults
      mode tcp
      log global
      option tcplog
      timeout connect 5s
      timeout client 12h
      timeout server 12h

    frontend ssh
      bind :22
      bind [::]:22 v6only
      default_backend service_ssh

    backend service_ssh
      server service_ssh service-ssh-sv.default.svc.cluster.local:2222 send-proxy
    %{if local.irc_enabled_bool}
    frontend irc
      bind :${local.irc_port}
      bind [::]:${local.irc_port} v6only
      default_backend irc

    backend irc
      server irc service-ssh-sv.default.svc.cluster.local:${local.irc_port} check inter 5s fall 2 rise 2${local.irc_proxy_emit_bool ? " send-proxy check-send-proxy" : ""}
    %{endif}
  EOT
}

resource "kubernetes_config_map_v1" "ssh_edge" {
  metadata {
    name      = "ssh-edge"
    namespace = "kube-system"
  }

  data = {
    "haproxy.cfg" = local.ssh_edge_haproxy_config
  }
}

resource "kubernetes_daemon_set_v1" "ssh_edge" {
  metadata {
    name      = "ssh-edge"
    namespace = "kube-system"

    labels = {
      app = "ssh-edge"
    }
  }

  spec {
    selector {
      match_labels = {
        app = "ssh-edge"
      }
    }

    template {
      metadata {
        labels = {
          app = "ssh-edge"
        }

        annotations = {
          config_hash = sha256(local.ssh_edge_haproxy_config)
        }
      }

      spec {
        host_network = true
        dns_policy   = "ClusterFirstWithHostNet"
        # Same window service-ssh drains for (service-ssh.tf): a replaced
        # edge pod keeps serving its open sessions until the last one ends
        # or this runs out.
        termination_grace_period_seconds = 21600

        node_selector = {
          "kubernetes.io/os" = "linux"
        }

        # A TUI session idles between frames, and by default any pause past
        # the retransmit timeout drops a connection back to slow start, so
        # the next big frame (a screen switch, a resize) waits extra round
        # trips to reach the user. Host network puts this pod in the node's
        # network namespace, so the setting covers every client-facing
        # socket on the node (this HAProxy, ipv6-proxy, LiveKit); the kernel
        # reads it live, so open connections get it too. SCALE.md, Pain
        # Point 11.
        init_container {
          name    = "tcp-tuning"
          image   = "haproxy:2.9-alpine"
          command = ["sysctl", "-w", "net.ipv4.tcp_slow_start_after_idle=0"]

          security_context {
            privileged  = true
            run_as_user = 0
          }
        }

        container {
          name    = "haproxy"
          image   = "haproxy:2.9-alpine"
          command = ["haproxy"]
          args    = ["-f", "/usr/local/etc/haproxy/haproxy.cfg", "-db"]

          # SIGTERM is HAProxy's hard stop; SIGUSR1 is the soft stop: close
          # the listeners, keep serving open sessions, exit after the last
          # one. The sleep holds back the kubelet's SIGTERM until then. When
          # HAProxy exits, the hook dies with the container, so a
          # FailedPreStopHook event on a drained pod is expected.
          lifecycle {
            pre_stop {
              exec {
                command = ["/bin/sh", "-c", "kill -USR1 1 && sleep 21600"]
              }
            }
          }

          # Legit edge load is a few millicores. The CPU cap keeps a port-22
          # flood (CONTEXT.md incident log, 2026-09-03) from taking the node,
          # and there is deliberately no liveness probe: a saturated proxy
          # slows down instead of being killed and dropping every session.
          resources {
            requests = {
              cpu    = "50m"
              memory = "64Mi"
            }
            limits = {
              cpu    = "1"
              memory = "512Mi"
            }
          }

          # Root so the surge pod's SO_REUSEPORT bind matches the old pod's
          # owner; the only capability kept is binding ports below 1024.
          security_context {
            allow_privilege_escalation = false
            run_as_non_root            = false
            run_as_user                = 0
            run_as_group               = 0

            capabilities {
              add  = ["NET_BIND_SERVICE"]
              drop = ["ALL"]
            }
          }

          volume_mount {
            name       = "config"
            mount_path = "/usr/local/etc/haproxy"
            read_only  = true
          }
        }

        volume {
          name = "config"

          config_map {
            name = kubernetes_config_map_v1.ssh_edge.metadata[0].name
          }
        }
      }
    }

    # hostNetwork with no declared hostPorts, so two pods can share the node
    # during the surge.
    strategy {
      type = "RollingUpdate"

      rolling_update {
        max_surge       = 1
        max_unavailable = 0
      }
    }
  }
}
