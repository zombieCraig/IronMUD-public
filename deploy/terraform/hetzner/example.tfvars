# Copy to terraform.tfvars and edit. Set the API token in the environment:
#   export HCLOUD_TOKEN=...

location = "nbg1"
name     = "ironmud"

# Required: SSH key names or IDs from your project (hcloud ssh-key list).
ssh_keys = ["my-key"]

# x86 "cx23" or ARM "cax11"; check what's orderable in your location.
server_type = "cx23"

# Daily backups of the whole server, game database included (+20%).
# backups = true

# Restrict SSH to your own address if you can.
# ssh_allowed_cidrs = ["203.0.113.7/32"]

# Pin a release instead of tracking the latest.
# ironmud_version = "v0.2.0"

# Optional TLS. After `apply`, point this domain's A record at the ipv4
# output within tls_dns_wait_seconds (30 min) and TLS turns on.
# tls_domain = "mud.example.com"
# tls_email  = "you@example.com"
# tls_only   = false
