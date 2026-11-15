terraform {
  # Terraform >= 1.6 or OpenTofu >= 1.6.
  required_version = ">= 1.6"

  required_providers {
    digitalocean = {
      source  = "digitalocean/digitalocean"
      version = ">= 2.40"
    }
  }
}
