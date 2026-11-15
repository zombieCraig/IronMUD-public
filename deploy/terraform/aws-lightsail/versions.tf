terraform {
  # Terraform >= 1.6 or OpenTofu >= 1.6.
  required_version = ">= 1.6"

  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = ">= 5.0"
    }
  }
}
