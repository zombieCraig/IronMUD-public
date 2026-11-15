# Copy to terraform.tfvars and edit. Set the API token in the environment:
#   export DIGITALOCEAN_TOKEN=dop_v1_...

region = "nyc3"
name   = "ironmud"

# Required: SSH keys from your DO account (doctl compute ssh-key list).
ssh_key_ids = ["12345678"]

# $4/mo 512 MB. Use "s-1vcpu-1gb" for 1 GB.
size = "s-1vcpu-512mb-10gb"

# Weekly snapshots of the whole droplet, game database included (+20%).
# backups = true

# Email alerts for CPU/memory/disk from the free monitoring agent.
# alert_emails = ["you@example.com"]

# Restrict SSH to your own address if you can.
# ssh_allowed_cidrs = ["203.0.113.7/32"]

# Pin a release instead of tracking the latest.
# ironmud_version = "v0.2.0"

# Optional TLS. After `apply`, point this domain's A record at the ipv4
# output within tls_dns_wait_seconds (30 min) and TLS turns on.
# tls_domain = "mud.example.com"
# tls_email  = "you@example.com"
# tls_only   = false
