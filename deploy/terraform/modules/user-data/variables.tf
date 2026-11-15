# Values land in a root-only settings file on the server; the validations keep
# them to plain tokens so nothing can break out of the generated script.

variable "ironmud_version" {
  description = "Release to install: \"latest\" or a tag such as \"v0.2.0\"."
  type        = string
  default     = "latest"
  validation {
    condition     = can(regex("^(latest|v[0-9]+\\.[0-9]+\\.[0-9]+([-+][0-9A-Za-z.-]+)?)$", var.ironmud_version))
    error_message = "ironmud_version must be \"latest\" or a tag like \"v0.2.0\"."
  }
}

variable "release_repo" {
  description = "GitHub owner/name that publishes IronMUD releases."
  type        = string
  default     = "zombieCraig/IronMUD-public"
  validation {
    condition     = can(regex("^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$", var.release_repo))
    error_message = "release_repo must look like owner/name."
  }
}

variable "tls_domain" {
  description = "Domain for TLS on port 4443 (Let's Encrypt). Empty = plaintext only."
  type        = string
  default     = ""
  validation {
    condition     = var.tls_domain == "" || can(regex("^([A-Za-z0-9]([A-Za-z0-9-]{0,61}[A-Za-z0-9])?\\.)+[A-Za-z]{2,63}$", var.tls_domain))
    error_message = "tls_domain must be a plain hostname such as mud.example.com."
  }
}

variable "tls_email" {
  description = "Optional Let's Encrypt account email for expiry notices."
  type        = string
  default     = ""
  validation {
    condition     = var.tls_email == "" || can(regex("^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\\.[A-Za-z]{2,63}$", var.tls_email))
    error_message = "tls_email must be a plain email address."
  }
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
  validation {
    condition     = var.tls_dns_wait_seconds >= 0 && var.tls_dns_wait_seconds <= 86400 && floor(var.tls_dns_wait_seconds) == var.tls_dns_wait_seconds
    error_message = "tls_dns_wait_seconds must be a whole number between 0 and 86400."
  }
}

variable "host_firewall" {
  description = "Also configure ufw on the server (in addition to the provider firewall)."
  type        = bool
  default     = true
}

variable "swap_mb" {
  description = "Swap file size added when the server has under 2 GB RAM and no swap (0 = never)."
  type        = number
  default     = 1024
  validation {
    condition     = var.swap_mb >= 0 && var.swap_mb <= 65536 && floor(var.swap_mb) == var.swap_mb
    error_message = "swap_mb must be a whole number between 0 and 65536."
  }
}
