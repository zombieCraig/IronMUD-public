module "user_data" {
  source = "../modules/user-data"

  ironmud_version      = var.ironmud_version
  release_repo         = var.release_repo
  tls_domain           = var.tls_domain
  tls_email            = var.tls_email
  tls_only             = var.tls_only
  tls_dns_wait_seconds = var.tls_dns_wait_seconds
  host_firewall        = var.host_firewall
  swap_mb              = var.swap_mb
}

# Inbound rules only: Hetzner firewalls allow all outbound traffic when no
# outbound rule is defined.
resource "hcloud_firewall" "ironmud" {
  name   = "${var.name}-fw"
  labels = var.labels

  rule {
    direction  = "in"
    protocol   = "tcp"
    port       = "22"
    source_ips = var.ssh_allowed_cidrs
  }

  dynamic "rule" {
    for_each = module.user_data.game_ports
    content {
      direction  = "in"
      protocol   = "tcp"
      port       = tostring(rule.value)
      source_ips = ["0.0.0.0/0", "::/0"]
    }
  }

  rule {
    direction  = "in"
    protocol   = "icmp"
    source_ips = ["0.0.0.0/0", "::/0"]
  }
}

resource "hcloud_server" "ironmud" {
  name         = var.name
  server_type  = var.server_type
  image        = var.image
  location     = var.location
  ssh_keys     = var.ssh_keys
  backups      = var.backups
  firewall_ids = [hcloud_firewall.ironmud.id]
  user_data    = module.user_data.script
  labels       = var.labels

  public_net {
    ipv4_enabled = true
    ipv6_enabled = var.ipv6
  }

  lifecycle {
    # User data only runs on first boot, and changing it (or the SSH keys or
    # image) would REPLACE the server and its game database. Change settings
    # on the server: edit /etc/ironmud/bootstrap.env, re-run
    # /root/ironmud-bootstrap.sh.
    ignore_changes = [user_data, ssh_keys, image]
  }
}
