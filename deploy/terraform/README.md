# IronMUD on a cloud server with Terraform / OpenTofu

One `apply` creates a small server, opens the right firewall ports and
installs the latest IronMUD release on first boot. It works with
[OpenTofu](https://opentofu.org) (`tofu`) or Terraform (`terraform`) 1.6+;
the commands are the same.

| Directory | Provider | Default size | Approx. price |
|---|---|---|---|
| `aws-lightsail/` | AWS Lightsail | `nano_3_0` (512 MB, static IP) | $5/mo |
| `digitalocean/` | DigitalOcean | `s-1vcpu-512mb-10gb` (free monitoring + alerts) | $4/mo |
| `hetzner/` | Hetzner Cloud | `cx23` (2 vCPU, 4 GB; or ARM `cax11`) | ~€5/mo |

Prices change; check the provider. 512 MB is enough: the bootstrap adds a
swap file on small servers.

## Quick start

```bash
cd deploy/terraform/digitalocean        # or aws-lightsail / hetzner
cp example.tfvars terraform.tfvars      # edit it
export DIGITALOCEAN_TOKEN=...           # HCLOUD_TOKEN for Hetzner; AWS uses your normal AWS credentials
tofu init
tofu apply
```

`apply` prints the server's IP, the SSH command and how players connect.
Setup continues on the server for a few minutes after `apply` finishes:

```bash
ssh root@<ip> tail -f /var/log/ironmud-bootstrap.log   # Lightsail: ssh ubuntu@<ip> sudo tail ...
ssh root@<ip> ironmud-admin health
```

You can also use a module straight from GitHub without cloning:

```hcl
module "mud" {
  source      = "git::https://github.com/zombieCraig/IronMUD-public.git//deploy/terraform/hetzner?ref=v0.2.0"
  ssh_keys    = ["my-key"]
  tls_domain  = "mud.example.com"
}
```

## TLS

Set `tls_domain` (and optionally `tls_email`). The server's IP isn't known
until `apply` finishes, so first boot waits up to `tls_dns_wait_seconds`
(30 minutes) for the domain to resolve:

1. `tofu apply`
2. Create an A record for the domain pointing at the printed IP (and an AAAA
   record for the IPv6 address if you like).
3. TLS turns on by itself on port 4443 with a Let's Encrypt certificate that
   renews automatically. Plaintext 4000 stays open meanwhile.

If the wait runs out, players still have port 4000; once DNS is in place run
`sudo bash /root/ironmud-bootstrap.sh` on the server.

`tls_only = true` closes port 4000 in the provider firewall, so players must
use TLS. Until TLS is working nobody can connect, so leave it off for your
first deploy if your DNS is slow.

## Changing settings later

User data only runs on the first boot, and changing it would replace the
server and **delete the game database**. The modules ignore later changes to
it (and to the image and SSH keys). To change IronMUD settings, edit the
server instead:

```bash
sudo nano /etc/ironmud/bootstrap.env      # e.g. set IRONMUD_TLS_DOMAIN
sudo bash /root/ironmud-bootstrap.sh
```

Firewall settings (`tls_domain`, `tls_only`, `ssh_allowed_cidrs`) are applied
by `tofu apply` as usual. Changing the size of a Lightsail instance
(`bundle_id`) replaces it; take a snapshot first.

Upgrading IronMUD: `sudo /opt/ironmud/install.sh --from-release` on the server.

## Backups

The game database lives on the server's disk at `/opt/ironmud/data`.
`tofu destroy` deletes it. Turn on provider backups (`backups = true` on
DigitalOcean and Hetzner; snapshots on Lightsail) or copy the data directory
off the server regularly.

## Monitoring

- IronMUD exposes `/healthz` and `/metrics` on `127.0.0.1:4002` (server-local);
  `ironmud-admin health` checks it.
- DigitalOcean: the module enables the free metrics agent; set `alert_emails`
  for CPU/memory/disk alerts.
- External uptime: any TCP port monitor (e.g. UptimeRobot's free plan) on
  port 4000 or 4443.
- Grafana: [`deploy/monitoring/`](../monitoring/) has a ready-made dashboard
  and an Alloy config for Grafana Cloud's free tier; setup steps are in the
  admin guide under "Grafana Cloud (free tier) with Alloy".

## What gets created

- **aws-lightsail:** instance, static IP, the instance's public ports (22,
  4000, plus 80/4443 with TLS).
- **digitalocean:** droplet (monitoring agent on), cloud firewall, optional
  monitor alerts.
- **hetzner:** server, firewall.

All of them pass the same first-boot script from `modules/user-data`, which
writes `/etc/ironmud/bootstrap.env` and runs the release's `bootstrap.sh`.
Don't put secrets in these variables: user data is readable from the
server's metadata service. Configure SMTP, Discord and API keys after boot.
