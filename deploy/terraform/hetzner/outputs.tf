output "ipv4" {
  description = "Public IPv4 address. Point tls_domain's A record here."
  value       = hcloud_server.ironmud.ipv4_address
}

output "ipv6" {
  description = "Public IPv6 address (if enabled). Optional AAAA record for tls_domain."
  value       = hcloud_server.ironmud.ipv6_address
}

output "connect" {
  description = "How players connect."
  value = compact([
    module.user_data.plaintext_open ? "telnet ${hcloud_server.ironmud.ipv4_address} 4000" : "",
    module.user_data.tls_enabled ? "TLS: ${lower(var.tls_domain)}:4443 (once DNS points at ${hcloud_server.ironmud.ipv4_address})" : "",
  ])
}

output "ssh" {
  description = "SSH command."
  value       = "ssh root@${hcloud_server.ironmud.ipv4_address}"
}

output "next_steps" {
  description = "What to run once the server is up."
  value       = <<-EOT
    Setup takes a few minutes after boot. Watch it:
      ssh root@${hcloud_server.ironmud.ipv4_address} tail -f /var/log/ironmud-bootstrap.log
    Check health:
      ssh root@${hcloud_server.ironmud.ipv4_address} ironmud-admin health
  EOT
}
