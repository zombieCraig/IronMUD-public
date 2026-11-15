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

resource "digitalocean_droplet" "ironmud" {
  name       = var.name
  region     = var.region
  size       = var.size
  image      = var.image
  ssh_keys   = var.ssh_key_ids
  ipv6       = var.ipv6
  backups    = var.backups
  monitoring = true # free metrics agent: CPU/memory/disk graphs and the alerts below
  user_data  = module.user_data.script
  tags       = var.tags

  lifecycle {
    # User data only runs on first boot, and changing it (or the SSH keys)
    # would REPLACE the droplet and its game database. Change settings on the
    # server: edit /etc/ironmud/bootstrap.env, re-run /root/ironmud-bootstrap.sh.
    ignore_changes = [user_data, ssh_keys, image]
  }
}

resource "digitalocean_firewall" "ironmud" {
  name        = "${var.name}-fw"
  droplet_ids = [digitalocean_droplet.ironmud.id]

  inbound_rule {
    protocol         = "tcp"
    port_range       = "22"
    source_addresses = var.ssh_allowed_cidrs
  }

  dynamic "inbound_rule" {
    for_each = module.user_data.game_ports
    content {
      protocol         = "tcp"
      port_range       = tostring(inbound_rule.value)
      source_addresses = ["0.0.0.0/0", "::/0"]
    }
  }

  inbound_rule {
    protocol         = "icmp"
    source_addresses = ["0.0.0.0/0", "::/0"]
  }

  # Outbound: apt, GitHub releases, Let's Encrypt, SMTP/Discord/Matrix bridges.
  outbound_rule {
    protocol              = "tcp"
    port_range            = "1-65535"
    destination_addresses = ["0.0.0.0/0", "::/0"]
  }

  outbound_rule {
    protocol              = "udp"
    port_range            = "1-65535"
    destination_addresses = ["0.0.0.0/0", "::/0"]
  }

  outbound_rule {
    protocol              = "icmp"
    destination_addresses = ["0.0.0.0/0", "::/0"]
  }
}

locals {
  alerts = {
    cpu    = { type = "v1/insights/droplet/cpu", value = 90, what = "CPU above 90%" }
    memory = { type = "v1/insights/droplet/memory_utilization_percent", value = 90, what = "memory above 90%" }
    disk   = { type = "v1/insights/droplet/disk_utilization_percent", value = 85, what = "disk above 85%" }
  }
}

resource "digitalocean_monitor_alert" "ironmud" {
  for_each = length(var.alert_emails) > 0 ? local.alerts : {}

  description = "${var.name}: ${each.value.what} for 10 minutes"
  type        = each.value.type
  compare     = "GreaterThan"
  value       = each.value.value
  window      = "10m"
  enabled     = true
  entities    = [digitalocean_droplet.ironmud.id]

  alerts {
    email = var.alert_emails
  }
}
