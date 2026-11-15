# Renders the first-boot script every provider module passes as user data,
# and the list of public game ports the provider firewall should open.
#
# It is a shell script rather than #cloud-config because Lightsail only
# documents shell launch scripts; cloud-init runs shell scripts everywhere.

terraform {
  required_version = ">= 1.6"
}

locals {
  tls_enabled = var.tls_domain != ""
  # TLS-only closes plaintext 4000, but only when there is a TLS port to use.
  plaintext_open = !(local.tls_enabled && var.tls_only)

  script = templatefile("${path.module}/user-data.sh.tftpl", {
    version      = var.ironmud_version
    release_repo = var.release_repo
    tls_domain   = lower(var.tls_domain)
    tls_email    = var.tls_email
    tls_only     = var.tls_only ? "1" : "0"
    dns_wait     = local.tls_enabled ? var.tls_dns_wait_seconds : 0
    firewall     = var.host_firewall ? "1" : "0"
    swap_mb      = var.swap_mb
  })

  # 80 is the Let's Encrypt HTTP-01 challenge (issue + renew).
  game_ports = concat(
    local.plaintext_open ? [4000] : [],
    local.tls_enabled ? [80, 4443] : [],
  )
}
