# Optional CI variables arrive as empty strings when the GitHub variable is
# unset. Normalize them here so Terraform remains the single source of defaults.
# App configuration does NOT live here: late-ssh reads one env (LATE_ENV=prod)
# plus secrets, and everything else is compiled into its prod profile
# (late-ssh/src/config.rs). These locals configure only infrastructure.
locals {
  # One resource spec for every door-game host pod (nethack, dcss, brogue,
  # dopewars, usurper, codekeep). They all have the same shape: an idle SSH
  # listener that forks one short-lived child per player.
  #
  # Requests are what actually reserve node capacity, and measured idle draw is
  # 1-2m CPU / 1-11Mi per host, so they stay small. Limits only cap bursts and
  # cost nothing until used, so they stay roomy enough for the heaviest door:
  # codekeep runs a Bun process per player (tens of MiB each) where the rest run
  # C binaries. Raise the limits here, never per door.
  door_cpu_request    = "50m"
  door_memory_request = "64Mi"
  door_cpu_limit      = "1000m"
  door_memory_limit   = "1Gi"

  # The root domain and public subdomains are code, not variables: changing a
  # hostname must land as a reviewed diff here and in the late-ssh/late-web
  # prod profiles together (they hardcode late.sh / rtc.late.sh).
  domain = "late.sh"

  livekit_subdomain           = "rtc"
  livekit_image               = trimspace(var.LIVEKIT_IMAGE) != "" ? trimspace(var.LIVEKIT_IMAGE) : "livekit/livekit-server:v1.9.12"
  livekit_log_level           = trimspace(var.LIVEKIT_LOG_LEVEL) != "" ? trimspace(var.LIVEKIT_LOG_LEVEL) : "info"
  livekit_api_key             = trimspace(var.LIVEKIT_API_KEY) != "" ? trimspace(var.LIVEKIT_API_KEY) : "late-voice"
  livekit_rtc_tcp_port        = tonumber(trimspace(var.LIVEKIT_RTC_TCP_PORT) != "" ? trimspace(var.LIVEKIT_RTC_TCP_PORT) : "7881")
  livekit_rtc_udp_port        = tonumber(trimspace(var.LIVEKIT_RTC_UDP_PORT) != "" ? trimspace(var.LIVEKIT_RTC_UDP_PORT) : "7882")
  livekit_rtc_use_external_ip = tobool(trimspace(var.LIVEKIT_RTC_USE_EXTERNAL_IP) != "" ? trimspace(var.LIVEKIT_RTC_USE_EXTERNAL_IP) : "true")
  livekit_turn_enabled        = tobool(trimspace(var.LIVEKIT_TURN_ENABLED) != "" ? trimspace(var.LIVEKIT_TURN_ENABLED) : "true")
  livekit_turn_udp_port       = tonumber(trimspace(var.LIVEKIT_TURN_UDP_PORT) != "" ? trimspace(var.LIVEKIT_TURN_UDP_PORT) : "3478")
  livekit_turn_tls_port       = tonumber(trimspace(var.LIVEKIT_TURN_TLS_PORT) != "" ? trimspace(var.LIVEKIT_TURN_TLS_PORT) : "5349")

  livekit_ingress_image     = trimspace(var.LIVEKIT_INGRESS_IMAGE) != "" ? trimspace(var.LIVEKIT_INGRESS_IMAGE) : "livekit/ingress:v1.4.3"
  livekit_whip_subdomain    = "whip"
  livekit_ingress_whip_port = tonumber(trimspace(var.LIVEKIT_INGRESS_WHIP_PORT) != "" ? trimspace(var.LIVEKIT_INGRESS_WHIP_PORT) : "7888")

  # IRC edge (ingress TCP passthrough, IPv6 HAProxy, certificate). The app
  # side is compiled into late-ssh's prod profile and always listens; these
  # gate only what the edge exposes. IRC_PROXY_EMIT stays a variable because
  # flipping edge emission is a deploy-time rollout step.
  irc_enabled_bool    = true
  irc_proxy_emit      = trimspace(var.IRC_PROXY_EMIT) != "" ? trimspace(var.IRC_PROXY_EMIT) : "0"
  irc_proxy_emit_bool = contains(["1", "true", "yes", "on"], lower(local.irc_proxy_emit))
  irc_host            = "irc.${local.domain}"
  irc_port            = 6697
  irc_tls_secret_name = "irc-tls"
  irc_tls_mount_path  = "/etc/irc-tls"
}
