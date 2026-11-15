terraform {
  # Terraform >= 1.6 or OpenTofu >= 1.6.
  required_version = ">= 1.6"

  required_providers {
    hcloud = {
      source  = "hetznercloud/hcloud"
      version = ">= 1.48"
    }
  }
}
