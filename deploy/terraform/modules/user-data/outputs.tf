output "script" {
  description = "First-boot script to pass as the server's user data."
  value       = local.script
}

output "game_ports" {
  description = "Public TCP ports the provider firewall should open to everyone (SSH is separate)."
  value       = local.game_ports
}

output "plaintext_open" {
  description = "Whether plaintext telnet on 4000 is reachable from outside."
  value       = local.plaintext_open
}

output "tls_enabled" {
  description = "Whether a TLS domain was configured."
  value       = local.tls_enabled
}
