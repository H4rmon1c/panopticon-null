# Self-hosted Panopticon deployment

Use a Linux VPS that you control, including a Namecheap VPS or a comparable provider. Namecheap can remain the DNS registrar. The public server serves only static files; the Rust collector and its SQLite/evidence store stay outside the web root. No ChatGPT Site, Vercel, or third-party app hosting is required. No VPS has been provisioned yet.

```text
official sources → scheduled Rust collector → private /var/lib/panopticon
                                             ↓ approved case export
                         /srv/panopticon/site/releases/<release>
                                             ↓ atomic current symlink
                                    Caddy → panopticon.fail
```

## What runs automatically today

`panopticon-collect.timer` starts 10 minutes after boot and then 25 hours after each completed collection. It invokes `collect.sh` to check the configured Colorado Springs Legistar events source and both procurement surfaces. The CLI enforces each source's reviewed scope, expiration, HTTPS/host rules, byte limits, and minimum request interval; unchanged and failed fetches are recorded according to the CLI's existing behavior. One failing source does not prevent checking the others, and the service reports failure for monitoring.

Collection writes to the private ledger. **The current atlas exporter supports only the approved Colorado Springs Ordinance 25-93 manifest. New council matters and procurement changes do not yet become public atlas cases automatically.** `panopticon-publish.service` is a deliberate one-shot release, with no timer. It copies only selected static assets, generates a fresh approved snapshot into a staged directory, validates it, then atomically switches the public symlink. A failure leaves the previous release live. It never copies the private preview, SQLite database, evidence store, or operator notes.

Source review is per source and expires; it is not a review task for every fetched record. An additional jurisdiction needs a configured source, adapter, and an expanded multi-case atlas exporter before it can appear publicly. Do not add arbitrary source IDs to the timer and expect the atlas to publish them.

## Provision a VPS

The commands below assume a Debian/Ubuntu Namecheap VPS with systemd, Caddy, Rust, Node 22, `flock`, and `realpath`. Install the runtime packages through your distribution or pinned release process. A 2 GB plan may be tight for compiling Rust and running PDF/OCR extraction on the same host; build the CLI off-host and copy the binary, or choose more memory. Build and install the Rust CLI from the reviewed branch:

```console
cargo build --locked --release -p pnull-cli
sudo install -m 0755 target/release/pnull /usr/local/bin/pnull
```

Create a dedicated unprivileged `panopticon` user and these paths:

```console
sudo useradd --system --home /var/lib/panopticon --shell /usr/sbin/nologin panopticon
sudo install -d -o panopticon -g panopticon -m 0700 /var/lib/panopticon
sudo install -d -o panopticon -g panopticon -m 0755 /srv/panopticon/repo /srv/panopticon/site /srv/panopticon/site/releases
sudo install -d -o root -g panopticon -m 0750 /etc/panopticon
```

Place the checked-out repository at `/srv/panopticon/repo`. Restore the approved collector state from an encrypted backup to `/var/lib/panopticon`, owned by `panopticon` with mode `0700`. The local `.pnull` directory contains the already recorded case approval and release allowlist; its database and evidence blobs are private operational state. A fresh empty store cannot export the case until that approval and allowlist are recorded. Do not put `.pnull` in the web tree or a public git repository. Back up the SQLite database, evidence blobs, source reviews, and release directories to encrypted off-host storage; test a restore before relying on the timer.

If you start with an empty private store instead of restoring `.pnull`, record
the approval already given for this exact Colorado Springs case in the new
store. Run `atlas requirements` against the **same manifest** first and use the
ID and binding it prints; do not copy an old binding from this guide:

```console
cd /srv/panopticon/repo
sudo -u panopticon /usr/local/bin/pnull --data-dir /var/lib/panopticon atlas requirements
sudo -u panopticon /usr/local/bin/pnull --data-dir /var/lib/panopticon atlas review \
  --id '<requirement-id-from-output>' --binding '<binding-from-output>' \
  --decision approve --reviewer h4rmon1c \
  --note 'Approved this exact Colorado Springs case and official sources in Codex chat on 2026-11-09'
sudo -u panopticon /usr/local/bin/pnull --data-dir /var/lib/panopticon atlas allow-release \
  --reviewer h4rmon1c --note 'Release the approved Colorado Springs atlas case'
```

If the manifest or its cited source digests differ from the approved case,
stop and obtain a new case-level approval before recording that review.

Install the example environment file and units, then edit the paths and domain:

```console
sudo install -o root -g panopticon -m 0640 deploy/selfhost/panopticon.env.example /etc/panopticon/panopticon.env
sudo install -m 0644 deploy/selfhost/panopticon-collect.service deploy/selfhost/panopticon-collect.timer deploy/selfhost/panopticon-publish.service /etc/systemd/system/
sudo systemctl daemon-reload
```

The example uses `PNUL_SITE_URL=https://panopticon.fail`. The released decision page currently has matching static social metadata. If the host changes, update that metadata and the configured URL together before publishing; the validator rejects a mismatch. `PNUL_CONFIG_PATH` and `PNUL_MANIFEST_PATH` are resolved from the repository working directory.

Install `web/panopticon-terminal/deploy/Caddyfile.example` as the Caddy site. Its root is `/srv/panopticon/site/current`, and it refuses the preview snapshot path. In Namecheap Advanced DNS, remove the existing apex URL Redirect/parking A record and parked `www` CNAME before the cutover; URL Redirect can create a hidden A record. Then point the domain's A record at **your VPS IP** (and add an AAAA only if the VPS has working IPv6), and let Caddy obtain TLS. Do not use the earlier ChatGPT Site DNS targets. The `panopticon` user must be able to write the release directory; the Caddy user needs read/traverse access only to `/srv/panopticon/site/current` and its release target. See [Namecheap's redirect-record instructions](https://www.namecheap.com/support/knowledgebase/article.aspx/385/2237/how-to-set-up-a-url-redirect-for-a-domain/).

## First release and daily checks

Before enabling collection, capture and record current source reviews as described in [`../../docs/operator-guide-source-review.md`](../../docs/operator-guide-source-review.md). The configured source reviews currently say they are not ready for unattended retrieval. The Legistar API currently returns HTTP 404 for `robots.txt`; the capture preserves that status and its bytes, and the reviewer must explicitly address it and the source's terms in the note before collection can begin. A 404 is not automatic permission. A review expires or becomes invalid if its source configuration changes; the collector then stops fetching that source and logs the reason.

```console
sudo systemctl start panopticon-publish.service
sudo systemctl status panopticon-publish.service
sudo systemctl enable --now panopticon-collect.timer
systemctl list-timers panopticon-collect.timer
journalctl -u panopticon-collect.service -n 100 --no-pager
```

Run `sudo systemctl start panopticon-publish.service` again after a reviewed case manifest or frontend release is ready. The publish service does not post to X. Keep database backups and a release rollback procedure: `current` is an atomic symlink, so an operator can point it back to a previously validated release if needed.

Run the deployment script's isolation and rollback test locally with `bash deploy/selfhost/test-publish.sh`. It uses a fake exporter and a temporary site; it does not touch production state.

## Work needed for continuous public expansion

1. Generalize the exporter from its hard-coded Colorado Springs manifest to a versioned directory of cases and a combined public snapshot. Give each case a stable ID and independently verifiable source bindings.
2. Build source adapters and scheduled discovery for each jurisdiction; persist new matters, votes, signed actions, awards, and revisions in the private ledger.
3. Convert detected changes into proposed evidence-backed cases and explicit gaps. Validate each connection against preserved bytes before it enters the public read model. Decide a release policy for newly generated cases; the current code requires one case-level approval.
4. Generate per-case share pages and social metadata from the released dataset, then add monitoring for collector failures, source-review expiry, and stale public snapshots.
