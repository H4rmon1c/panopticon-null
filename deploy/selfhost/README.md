# Self-hosted Panopticon deployment

Use a Linux VPS that you control, including a Namecheap VPS or a comparable provider. Namecheap can remain the DNS registrar. The public server serves only static files; the Rust collector and its SQLite/evidence store stay outside the web root. No ChatGPT Site, Vercel, or third-party app hosting is required. No VPS has been provisioned yet.

```text
official sources → scheduled Rust collector → private /var/lib/panopticon
                                             ↓ verified case export
                         /srv/panopticon/site/releases/<release>
                                             ↓ atomic current symlink
                                    Caddy → panopticon.fail
```

## What runs automatically today

`panopticon-collect.timer` starts 10 minutes after boot and then 25 hours after each completed collection. It invokes `collect.sh`, which reads the validated [collection matrix](collection-sources.tsv). The matrix schedules seven official HTML meeting calendars, plus the Colorado Springs procurement mirrors. A separate [fallback matrix](collection-fallbacks.tsv) invokes one of five configured Legistar event APIs only if its corresponding calendar fails and the API has a current source review; that collection run still reports degraded status. The CLI enforces each source's reviewed scope, expiration, HTTPS/host rules, byte limits, and minimum request interval. A source without a current review is skipped and makes the service report failure for monitoring; the remaining sources are still checked. Add each new jurisdiction's official source as a separate config and matrix row rather than reusing another city's source ID or jurisdiction. See [document-first discovery](../../docs/document-first-discovery.md).

Collection writes to the private ledger. The HTML calendar collector preserves meeting, agenda, and minutes link candidates privately. An optional, separately reviewed [candidate queue](candidate-queues.tsv) can follow those links into private raw-document archives with bounded requests and durable receipts; its matrix is currently empty and its timer is not enabled. Neither collector generates public case facts. `panopticon-publish.service` is a deliberate one-shot release, with no timer. It copies only the files listed in `public-assets.txt`, generates a fresh approved snapshot into a staged directory, validates it, then atomically switches the public symlink. A failure leaves the previous release live. When adding a browser asset, add its exact path to that list; arbitrary files in an asset directory do not enter a release. The private preview, SQLite database, evidence store, and operator notes stay outside the public release.

Source review is per source and expires; it is not a review task for every fetched record. Scheduling a source does not publish its findings. Additional jurisdictions need official source adapters, preserved evidence, and a verified atlas case before a decision appears publicly. The coverage map may show a registered jurisdiction with no indexed case.

The browser and public Caddy server make no Legistar, Census, or DOLA API calls. Official documents and API responses used as evidence are archived locally with SHA-256 digests. [Direct-document archiving](../../docs/colorado-direct-archive.md) supports 39 fixed municipal HTML, PDF, and JSON endpoints through the same bounded, DNS-safe private fetcher. These document endpoints are configured but not in the active matrix until their source reviews are recorded. The scheduled calendars are HTML pages; each link remains a private candidate until its document is archived and independently verified.

## Provision a VPS

The selected target is a [Namecheap Quasar VPS](https://www.namecheap.com/hosting/vps/) (4 CPU cores, 6 GB RAM, 120 GB SSD). Choose **Ubuntu 24.04 Blank**, **no web panel**, and **User-Responsible** management. The blank OS lets Caddy serve the static site without cPanel; User-Responsible retains the root access needed to install the collector and systemd units. [Namecheap lists Ubuntu 24.04 Blank](https://www.namecheap.com/support/knowledgebase/article.aspx/9979/2188/what-operating-systems-are-available-with-a-vpsdedicated-server/) and says [Complete management does not provide root access](https://www.namecheap.com/support/knowledgebase/article.aspx/9304/48/what-vps-management-options-do-you-provide/). Review the billing term and renewal price in Namecheap checkout before paying. No VPS order has been placed by this project.

After activation, obtain the public IPv4 address from Namecheap's Hosting List and establish SSH key access for a non-root administrator. Keep the initial root password in your password manager; do not send it in chat. Keep DNS pointing at parking until the static release and server services pass local checks; then switch the A record and verify HTTPS. Namecheap's [VPS setup guide](https://www.namecheap.com/support/knowledgebase/article.aspx/10777/48/how-to-set-up-a-vps-beginnerfriendly-stepbystep-guide/) covers the first login, system updates, SSH keys, and firewall rules for ports 22, 80, and 443.

The commands below assume a Debian/Ubuntu VPS with systemd, Caddy, Rust, Node 22, `flock`, and `realpath`. Install the runtime packages through your distribution or pinned release process. Build and install the Rust CLI from the reviewed branch:

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

Before enabling collection, prove that the PDF sandbox works as the service
user on this VPS. Build the ignored integration test, then run its binary under
`panopticon` from the repository directory:

```console
cargo test --locked -p pnull-ingest --test real_sandbox_pdf --no-run
test_bin="$(find target/debug/deps -maxdepth 1 -type f -name 'real_sandbox_pdf-*' -perm /111 -print -quit)"
test -n "$test_bin"
sudo -u panopticon "$PWD/$test_bin" --ignored --nocapture
```

The test must extract the fixture through Bubblewrap and Poppler. Ubuntu 24.04
can restrict unprivileged user namespaces through AppArmor; if the test fails
at sandbox setup, inspect the denial and configure a narrow profile for
Bubblewrap before retrying. Keep the system-wide namespace restriction in
place. See [Ubuntu's 24.04 security notes](https://documentation.ubuntu.com/release-notes/24.04/#unprivileged-user-namespace-restrictions)
and [AppArmor guidance](https://documentation.ubuntu.com/security/security-features/privilege-restriction/apparmor/).

The example uses `PNUL_SITE_URL=https://panopticon.fail`. The released decision page currently has matching static social metadata. If the host changes, update that metadata and the configured URL together before publishing; the validator rejects a mismatch. `PNUL_CONFIG_PATH` and `PNUL_MANIFEST_PATH` are resolved from the repository working directory. The default manifest is the already approved Colorado Springs case. Once each of the 12 release-cited sources for the four new cases has a current review and matching private archive capture, set `PNUL_MANIFEST_PATHS` to a space-separated list of the five manifest paths under `fixtures/atlas/`. Five historical supporting attachments on robots-disallowed hosts are not release citations and should not be scheduled for automated fetch. The exporter will refuse an incomplete bundle; the publish script leaves the prior release live. See [the Colorado coverage inventory](../../docs/colorado-coverage.md) for the exact case filenames and attribution limits.

Install `web/panopticon-terminal/deploy/Caddyfile.example` as the Caddy site. Its root is `/srv/panopticon/site/current`, and it refuses the preview snapshot path. In Namecheap Advanced DNS, remove the existing apex URL Redirect/parking A record and parked `www` CNAME before the cutover; URL Redirect can create a hidden A record. Then point the domain's A record at **your VPS IP** (and add an AAAA only if the VPS has working IPv6), and let Caddy obtain TLS. Do not use the earlier ChatGPT Site DNS targets. The `panopticon` user must be able to write the release directory; the Caddy user needs read/traverse access only to `/srv/panopticon/site/current` and its release target. See [Namecheap's redirect-record instructions](https://www.namecheap.com/support/knowledgebase/article.aspx/385/2237/how-to-set-up-a-url-redirect-for-a-domain/).

## First release and daily checks

Before enabling collection, capture and record current source reviews as described in [`../../docs/operator-guide-source-review.md`](../../docs/operator-guide-source-review.md). The configured source reviews currently say they are not ready for unattended retrieval. If any host returns HTTP 404 for `robots.txt`, the capture preserves that status and its bytes, and the reviewer must explicitly address it and the source's terms in the note before collection can begin. A 404 is not automatic permission. A review expires or becomes invalid if its source configuration changes; the collector then stops fetching that source and logs the reason.

```console
sudo systemctl start panopticon-publish.service
sudo systemctl status panopticon-publish.service
sudo systemctl enable --now panopticon-collect.timer
systemctl list-timers panopticon-collect.timer
journalctl -u panopticon-collect.service -n 100 --no-pager
```

Run `sudo systemctl start panopticon-publish.service` again after a reviewed case manifest or frontend release is ready. The publish service does not post to X. Keep database backups and a release rollback procedure: `current` is an atomic symlink, so an operator can point it back to a previously validated release if needed.

Run `bash deploy/selfhost/test-collect.sh` to check profile routing and source-review failure isolation, `bash deploy/selfhost/test-candidate-queue.sh` to check bounded queue routing and matrix validation, then `bash deploy/selfhost/test-publish.sh` to check publication isolation and rollback. They use temporary state and fake CLI/exporter commands; none touches production state. Once the calendar and matching document-path reviews are recorded, add exact rows to `candidate-queues.tsv`; only then install and enable `panopticon-candidates.service` and `panopticon-candidates.timer`. Both collectors share a lock so the queue reads complete candidate artifacts.

## Work needed for continuous public expansion

1. Review and archive the 12 release-cited official endpoints used by the four new Colorado cases. The version 2 exporter checks their exact locators, source digests, current reviews, private fetch receipts, and raw blobs; it does not require a human decision for each case. An explicit rejection or correction blocks release. Obtain city-hosted alternatives or direct public-record copies for the five robots-disallowed supporting attachments before considering them for automated collection.
2. Expand reviewed official document-index discovery beyond the seven configured Colorado calendars. The bounded private queue follows preserved candidates after origin and path-policy reviews; it does not discover older meetings absent from the current calendar or publish a case. Add historical calendar paging, broader official source profiles, and a verified route from private case proposals to publication. Require complete evidence chains before publication.
3. Add official role and term evidence for durable officeholder profiles, correction/withdrawal history for individual facts, and coverage monitoring for all indexed jurisdictions. Consent-block roll calls already appear as motion context without being attributed to each item.
4. Verify source-review expiry, stale public snapshots, backup/restore, Caddy TLS, and release rollback on the actual VPS. The static exporter and per-decision share-page generator are ready for this check once source provenance is captured.
