# --- Server -------------------------------------------------------------------

variable "region" {
  description = "AWS region, e.g. us-east-1."
  type        = string
  default     = "us-east-1"
}

variable "availability_zone" {
  description = "Lightsail availability zone. Empty = the region's \"a\" zone."
  type        = string
  default     = ""
}

variable "name" {
  description = "Instance name; also prefixes the static IP."
  type        = string
  default     = "ironmud"
}

variable "bundle_id" {
  description = "Lightsail plan. nano_3_0 = 512 MB ($5/mo, bootstrap adds swap); micro_3_0 = 1 GB."
  type        = string
  default     = "nano_3_0"
}

variable "blueprint_id" {
  description = "OS image. Ubuntu 22.04+ or Debian 12+."
  type        = string
  default     = "ubuntu_24_04"
}

variable "key_pair_name" {
  description = "Existing Lightsail key pair for SSH. Empty = the region's default key pair."
  type        = string
  default     = ""
}

variable "ipv6" {
  description = "Give the instance an IPv6 address as well (dual-stack)."
  type        = bool
  default     = true
}

variable "ssh_allowed_cidrs" {
  description = "Networks allowed to SSH (port 22). IPv4 and IPv6 CIDRs may be mixed."
  type        = list(string)
  default     = ["0.0.0.0/0", "::/0"]
}

variable "tags" {
  description = "Tags for the instance and static IP."
  type        = map(string)
  default     = {}
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
  description = "Domain for TLS on 4443 via Let's Encrypt; point its DNS at the static_ip output. Empty = plaintext only."
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
  description = "Also configure ufw on the server (in addition to the Lightsail firewall)."
  type        = bool
  default     = true
}

variable "swap_mb" {
  description = "Swap file size added when the server has under 2 GB RAM (0 = never)."
  type        = number
  default     = 1024
}
