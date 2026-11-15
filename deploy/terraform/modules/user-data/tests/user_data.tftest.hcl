# Run with: tofu test   (or terraform test) from modules/user-data

run "plaintext_by_default" {
  command = plan

  assert {
    condition     = jsonencode(output.game_ports) == jsonencode([4000])
    error_message = "default should open only plaintext 4000"
  }
  assert {
    condition     = startswith(output.script, "#!/bin/bash\n")
    error_message = "user data must be a shell script"
  }
  assert {
    condition     = strcontains(output.script, "IRONMUD_TLS_DOMAIN=\n") && strcontains(output.script, "IRONMUD_TLS_DNS_WAIT=0\n")
    error_message = "no domain should mean no TLS and no DNS wait"
  }
  assert {
    condition     = strcontains(output.script, "https://github.com/zombieCraig/IronMUD-public/releases/latest/download/bootstrap.sh")
    error_message = "latest should use the releases/latest URL"
  }
}

run "tls_opens_80_and_4443" {
  command = plan
  variables {
    tls_domain = "Mud.Example.com"
    tls_email  = "alice@example.com"
  }

  assert {
    condition     = jsonencode(output.game_ports) == jsonencode([4000, 80, 4443])
    error_message = "TLS should add 80 and 4443 and keep 4000"
  }
  assert {
    condition     = strcontains(output.script, "IRONMUD_TLS_DOMAIN=mud.example.com\n") && strcontains(output.script, "IRONMUD_TLS_DNS_WAIT=1800\n")
    error_message = "domain should be lower-cased and the DNS wait set"
  }
}

run "tls_only_closes_4000" {
  command = plan
  variables {
    tls_domain = "mud.example.com"
    tls_only   = true
  }

  assert {
    condition     = jsonencode(output.game_ports) == jsonencode([80, 4443]) && !output.plaintext_open
    error_message = "tls_only should close 4000"
  }
}

run "tls_only_without_domain_keeps_4000" {
  command = plan
  variables {
    tls_only = true
  }

  assert {
    condition     = jsonencode(output.game_ports) == jsonencode([4000])
    error_message = "tls_only without a domain must not lock players out"
  }
}

run "pinned_version_url" {
  command = plan
  variables {
    ironmud_version = "v0.2.0"
  }

  assert {
    condition     = strcontains(output.script, "/releases/download/v0.2.0/bootstrap.sh")
    error_message = "a tag should use the releases/download/<tag> URL"
  }
}

run "rejects_shell_metacharacters_in_domain" {
  command = plan
  variables {
    tls_domain = "mud.example.com\nIRONMUD_FIREWALL=0"
  }
  expect_failures = [var.tls_domain]
}

run "rejects_bad_version" {
  command = plan
  variables {
    ironmud_version = "main; rm -rf /"
  }
  expect_failures = [var.ironmud_version]
}

run "rejects_bad_email" {
  command = plan
  variables {
    tls_email = "a@b.com$(reboot)"
  }
  expect_failures = [var.tls_email]
}
