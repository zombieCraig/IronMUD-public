# Copy to terraform.tfvars and edit. Only region is commonly changed;
# everything else has a working default.

region = "us-east-1"
name   = "ironmud"

# 512 MB ($5/mo). Use "micro_3_0" for 1 GB.
bundle_id = "nano_3_0"

# Optional: your own Lightsail key pair (empty = region default key).
# key_pair_name = "my-key"

# Restrict SSH to your own address if you can.
# ssh_allowed_cidrs = ["203.0.113.7/32"]

# Pin a release instead of tracking the latest.
# ironmud_version = "v0.2.0"

# Optional TLS. After `apply`, point this domain's A record at the
# static_ip output within tls_dns_wait_seconds (30 min) and TLS turns on.
# tls_domain = "mud.example.com"
# tls_email  = "you@example.com"
# tls_only   = false
