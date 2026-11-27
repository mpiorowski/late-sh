# Keep these declarations until production state has forgotten the old
# IPv6-only TCP proxy. It must be deleted manually during the maintenance
# cutover, before host-network ingress-nginx binds the same IPv6 ports.
# destroy=false ensures an ordinary Terraform apply never deletes the live
# proxy outside that window; the no-destroys PR gate remains intact.
removed {
  from = kubernetes_daemon_set_v1.ipv6_proxy

  lifecycle {
    destroy = false
  }
}

removed {
  from = kubernetes_config_map_v1.ipv6_proxy

  lifecycle {
    destroy = false
  }
}
