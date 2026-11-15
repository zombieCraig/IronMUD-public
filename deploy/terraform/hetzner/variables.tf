# The provider reads the API token from the HCLOUD_TOKEN environment
# variable; keep it out of tfvars files.

# --- Server -------------------------------------------------------------------

variable "location" {
  description = "Hetzner location: nbg1, fsn1, hel1 (EU), ash, hil (US), sin."
  type        = string
  default     = "nbg1"
}

variable "name" {
  description = "Server name; also prefixes the firewall."
  type        = string
  default     = "ironmud"
}

variable "server_type" {
  description = "Server type. cx23 (x86) or cax11 (ARM; releases ship aarch64 too). Availability varies by location: `hcloud server-type list`."
  type        = string
  default     = "cx23"
}

variable "image" {
  description = "OS image. Ubuntu 22.04+ or Debian 12+."
  type        = string
  default     = "ubuntu-24.04"
}

variable "ssh_keys" {
  description = "Names or IDs of SSH keys already in your Hetzner project (`hcloud ssh-key list`)."
  type        = list(string)
  validation {
    condition     = length(var.ssh_keys) > 0
    error_message = "Add at least one SSH key; without one Hetzner emails a root password instead."
  }
}

variable "ipv6" {
  description = "Give the server a public IPv6 address."
  type        = bool
  default     = true
}

variable "backups" {
  description = "Enable Hetzner backups (+20% of the server price). Recommended: they include the game database."
  type        = bool
  default     = false
}

variable "ssh_allowed_cidrs" {
  description = "Networks allowed to SSH (port 22). IPv4 and IPv6 CIDRs may be mixed."
  type        = list(string)
  default     = ["0.0.0.0/0", "::/0"]
}

variable "labels" {
  description = "Labels for the server and firewall."
  type        = map(string)
  default     = { app = "ironmud" }
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
  description = "Also configure ufw on the server (in addition to the Hetzner firewall)."
  type        = bool
  default     = true
}

variable "swap_mb" {
  description = "Swap file size added when the server has under 2 GB RAM (0 = never)."
  type        = number
  default     = 1024
}
