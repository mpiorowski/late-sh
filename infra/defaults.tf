# Optional CI variables arrive as empty strings when the GitHub variable is
# unset. Normalize them here so Terraform remains the single source of defaults.
locals {
  rebels_enabled = trimspace(var.REBELS_ENABLED) != "" ? trimspace(var.REBELS_ENABLED) : "1"
  rebels_host    = trimspace(var.REBELS_HOST) != "" ? trimspace(var.REBELS_HOST) : "frittura.org"
  rebels_port    = trimspace(var.REBELS_PORT) != "" ? trimspace(var.REBELS_PORT) : "3788"

  # dopewars is a local-PTY child of service-ssh (no separate host/PVC/secret),
  # so this is the only knob it needs beyond the baked-in binary path.
  dopewars_enabled = trimspace(var.DOPEWARS_ENABLED) != "" ? trimspace(var.DOPEWARS_ENABLED) : "1"

  voice_enabled = trimspace(var.VOICE_ENABLED) != "" ? trimspace(var.VOICE_ENABLED) : "1"
  voice_room    = trimspace(var.VOICE_ROOM) != "" ? trimspace(var.VOICE_ROOM) : "late-voice"

  livekit_subdomain           = trimspace(var.LIVEKIT_SUBDOMAIN) != "" ? trimspace(var.LIVEKIT_SUBDOMAIN) : "rtc"
  livekit_image               = trimspace(var.LIVEKIT_IMAGE) != "" ? trimspace(var.LIVEKIT_IMAGE) : "livekit/livekit-server:v1.9.12"
  livekit_log_level           = trimspace(var.LIVEKIT_LOG_LEVEL) != "" ? trimspace(var.LIVEKIT_LOG_LEVEL) : "info"
  livekit_api_key             = trimspace(var.LIVEKIT_API_KEY) != "" ? trimspace(var.LIVEKIT_API_KEY) : "late-voice"
  livekit_rtc_tcp_port        = tonumber(trimspace(var.LIVEKIT_RTC_TCP_PORT) != "" ? trimspace(var.LIVEKIT_RTC_TCP_PORT) : "7881")
  livekit_rtc_udp_port        = tonumber(trimspace(var.LIVEKIT_RTC_UDP_PORT) != "" ? trimspace(var.LIVEKIT_RTC_UDP_PORT) : "7882")
  livekit_rtc_use_external_ip = tobool(trimspace(var.LIVEKIT_RTC_USE_EXTERNAL_IP) != "" ? trimspace(var.LIVEKIT_RTC_USE_EXTERNAL_IP) : "true")
  livekit_turn_enabled        = tobool(trimspace(var.LIVEKIT_TURN_ENABLED) != "" ? trimspace(var.LIVEKIT_TURN_ENABLED) : "true")
  livekit_turn_udp_port       = tonumber(trimspace(var.LIVEKIT_TURN_UDP_PORT) != "" ? trimspace(var.LIVEKIT_TURN_UDP_PORT) : "3478")
  livekit_turn_tls_port       = tonumber(trimspace(var.LIVEKIT_TURN_TLS_PORT) != "" ? trimspace(var.LIVEKIT_TURN_TLS_PORT) : "5349")

  irc_enabled                  = trimspace(var.IRC_ENABLED) != "" ? trimspace(var.IRC_ENABLED) : "0"
  irc_enabled_bool             = contains(["1", "true", "yes", "on"], lower(local.irc_enabled))
  irc_host                     = trimspace(var.IRC_HOST) != "" ? trimspace(var.IRC_HOST) : "irc.${var.DOMAIN}"
  irc_port                     = tonumber(trimspace(var.IRC_PORT) != "" ? trimspace(var.IRC_PORT) : "6697")
  irc_max_conns_global         = trimspace(var.IRC_MAX_CONNS_GLOBAL) != "" ? trimspace(var.IRC_MAX_CONNS_GLOBAL) : "200"
  irc_max_conns_per_user       = trimspace(var.IRC_MAX_CONNS_PER_USER) != "" ? trimspace(var.IRC_MAX_CONNS_PER_USER) : "3"
  irc_max_auth_failures_per_ip = trimspace(var.IRC_MAX_AUTH_FAILURES_PER_IP) != "" ? trimspace(var.IRC_MAX_AUTH_FAILURES_PER_IP) : "20"
  irc_auth_failure_window_secs = trimspace(var.IRC_AUTH_FAILURE_WINDOW_SECS) != "" ? trimspace(var.IRC_AUTH_FAILURE_WINDOW_SECS) : "300"
  irc_tls_secret_name          = "irc-tls"
  irc_tls_mount_path           = "/etc/irc-tls"
}
