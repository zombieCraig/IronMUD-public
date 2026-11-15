provider "aws" {
  region = var.region
}

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

locals {
  ssh_ipv4 = [for c in var.ssh_allowed_cidrs : c if !strcontains(c, ":")]
  ssh_ipv6 = [for c in var.ssh_allowed_cidrs : c if strcontains(c, ":")]
}

resource "aws_lightsail_instance" "ironmud" {
  name              = var.name
  availability_zone = var.availability_zone != "" ? var.availability_zone : "${var.region}a"
  blueprint_id      = var.blueprint_id
  bundle_id         = var.bundle_id
  key_pair_name     = var.key_pair_name != "" ? var.key_pair_name : null
  ip_address_type   = var.ipv6 ? "dualstack" : "ipv4"
  user_data         = module.user_data.script
  tags              = var.tags

  lifecycle {
    # User data only runs on first boot, and changing it would REPLACE the
    # instance (and its game database). Change settings on the server instead:
    # edit /etc/ironmud/bootstrap.env and re-run /root/ironmud-bootstrap.sh.
    ignore_changes = [user_data, key_pair_name, blueprint_id]
  }
}

# A static IP survives stop/start, so DNS for tls_domain stays valid.
resource "aws_lightsail_static_ip" "ironmud" {
  name = "${var.name}-ip"
}

resource "aws_lightsail_static_ip_attachment" "ironmud" {
  static_ip_name = aws_lightsail_static_ip.ironmud.name
  instance_name  = aws_lightsail_instance.ironmud.name
}

# Lightsail's firewall. This resource owns ALL of the instance's public
# ports: anything not listed here is closed.
resource "aws_lightsail_instance_public_ports" "ironmud" {
  instance_name = aws_lightsail_instance.ironmud.name

  port_info {
    protocol   = "tcp"
    from_port  = 22
    to_port    = 22
    cidrs      = local.ssh_ipv4
    ipv6_cidrs = var.ipv6 ? local.ssh_ipv6 : null
  }

  dynamic "port_info" {
    for_each = module.user_data.game_ports
    content {
      protocol   = "tcp"
      from_port  = port_info.value
      to_port    = port_info.value
      cidrs      = ["0.0.0.0/0"]
      ipv6_cidrs = var.ipv6 ? ["::/0"] : null
    }
  }
}
