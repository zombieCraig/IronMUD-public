# The provider reads the API token from the DIGITALOCEAN_TOKEN environment
# variable; keep it out of tfvars files.

# --- Server -------------------------------------------------------------------

variable "region" {
  description = "Droplet region slug, e.g. nyc3, sfo3, ams3, fra1, sgp1."
  type        = string
  default     = "nyc3"
}

variable "name" {
  description = "Droplet name; also prefixes the firewall and alerts."
  type        = string
  default     = "ironmud"
}

variable "size" {
  description = "Droplet size. s-1vcpu-512mb-10gb = $4/mo (bootstrap adds swap); s-1vcpu-1gb = $6/mo."
  type        = string
  default     = "s-1vcpu-512mb-10gb"
}

variable "image" {
  description = "OS image slug. Ubuntu 22.04+ or Debian 12+."
  type        = string
  default     = "ubuntu-24-04-x64"
}

variable "ssh_key_ids" {
  description = "IDs or fingerprints of SSH keys already added to your DigitalOcean account (`doctl compute ssh-key list`)."
  type        = list(string)
  validation {
    condition     = length(var.ssh_key_ids) > 0
    error_message = "Add at least one SSH key; without one DigitalOcean emails a root password instead."
  }
}

variable "ipv6" {
  description = "Enable IPv6 on the droplet."
  type        = bool
  default     = true
}

variable "backups" {
  description = "Enable DigitalOcean weekly backups (+20% of the droplet price). Recommended: they include the game database."
  type        = bool
  default     = false
}

variable "ssh_allowed_cidrs" {
  description = "Networks allowed to SSH (port 22). IPv4 and IPv6 CIDRs may be mixed."
  type        = list(string)
  default     = ["0.0.0.0/0", "::/0"]
}

variable "alert_emails" {
  description = "Email addresses for CPU/memory/disk alerts from the free monitoring agent. Empty = no alerts."
  type        = list(string)
  default     = []
}

variable "tags" {
  description = "Droplet tags."
  type        = list(string)
  default     = ["ironmud"]
}

# --- IronMUD (passed to modules/user-data, which validates them) -------------

variable "ironmud_version" {
  description = "Release to install: \"latest\" or a tag such as \"v0.2.0\"."
  type        = string
  default     = "latest"
}

variable "release_repo" {
  description = "GitHub owner/name that publishes IronMUD releases."
  type        = string
  default     = "zombieCraig/IronMUD-public"
}

variable "tls_domain" {
  description = "Domain for TLS on 4443 via Let's Encrypt; point its DNS at the ipv4 output. Empty = plaintext only."
  type        = string
  default     = ""
}

variable "tls_email" {
  description = "Optional Let's Encrypt account email for expiry notices."
  type        = string
  default     = ""
}

variable "tls_only" {
  description = "Close plaintext port 4000 so players must use TLS. Ignored without tls_domain."
  type        = bool
  default     = false
}

variable "tls_dns_wait_seconds" {
  description = "How long first boot waits for tls_domain to resolve before skipping TLS."
  type        = number
  default     = 1800
}

variable "host_firewall" {
  description = "Also configure ufw on the server (in addition to the cloud firewall)."
  type        = bool
  default     = true
}

variable "swap_mb" {
  description = "Swap file size added when the server has under 2 GB RAM (0 = never)."
  type        = number
  default     = 1024
}
