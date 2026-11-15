output "static_ip" {
  description = "Public IPv4 address. Point tls_domain's A record here."
  value       = aws_lightsail_static_ip.ironmud.ip_address
}

output "ipv6_addresses" {
  description = "Public IPv6 addresses (dual-stack only). Optional AAAA record for tls_domain."
  value       = aws_lightsail_instance.ironmud.ipv6_addresses
}

output "connect" {
  description = "How players connect."
  value = compact([
    module.user_data.plaintext_open ? "telnet ${aws_lightsail_static_ip.ironmud.ip_address} 4000" : "",
    module.user_data.tls_enabled ? "TLS: ${lower(var.tls_domain)}:4443 (once DNS points at ${aws_lightsail_static_ip.ironmud.ip_address})" : "",
  ])
}

output "ssh" {
  description = "SSH command (Lightsail's default key pair unless key_pair_name was set)."
  value       = "ssh ubuntu@${aws_lightsail_static_ip.ironmud.ip_address}"
}

output "next_steps" {
  description = "What to run once the server is up."
  value       = <<-EOT
    Setup takes a few minutes after boot. Watch it:
      ssh ubuntu@${aws_lightsail_static_ip.ironmud.ip_address} sudo tail -f /var/log/ironmud-bootstrap.log
    Check health:
      ssh ubuntu@${aws_lightsail_static_ip.ironmud.ip_address} ironmud-admin health
  EOT
}
