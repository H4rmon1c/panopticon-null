# Colorado production commissioning and recovery

Prepared November 10, 2026 for the purchased Namecheap Magnetar VPS. No IP or SSH access is available yet. The package has been exercised locally; this document does not claim the host is installed or protected against every advanced attacker.

## Host installation

1. Verify the delivered OS and architecture. Use a supported Linux distribution with systemd, Caddy, Node 22+, Python 3, Bubblewrap, Poppler (`pdfinfo`, `pdftotext`, and `pdftoppm`), Tesseract with English data, `prlimit`, `flock`, and the reviewed Rust `pnull` binary. Build from the pinned repository revision and Cargo lockfile, or transfer a verified matching-architecture release binary. Install the binary root-owned at `/usr/local/bin/pnull`, mode 0755. The research extractor searches `/usr/bin:/bin` unless `PNUL_EXTRACTOR_PATH` is explicitly configured. If English data is outside Tesseract's system default, configure `PNUL_TESSDATA_DIR` to its directory under `/usr` or `/nix/store`.
2. Establish a separate administrator with SSH key access and sudo, retain an emergency provider console, apply OS security updates, and permit SSH only from the administrative network where practical. Expose only HTTP/HTTPS publicly. Confirm a second SSH login before changing SSH authentication or firewall rules. No database or collector port is needed.
3. Install only tracked repository files at `/srv/panopticon/repo`; keep the entire checkout root-owned and nonwritable by the service user. Do not copy an entire development directory with `.pnull`, environment files, or unrelated credentials. Run `sudo bash deploy/selfhost/install-host.sh` there. It creates protected directories and installs units plus `/etc/caddy/Caddyfile.panopticon`; it does not enable timers or replace an existing live Caddy config.
4. Transfer the approved private production store through an encrypted channel to `/var/lib/panopticon`, owned by `panopticon`, directory mode 0700 and private file mode 0600. Place the separately captured statewide research store at `/var/lib/panopticon/research`; its archive is not contained in the Git checkout. Preserve the existing review/allowlist/capture records. Do not manufacture new review decisions in a fresh store to make a failed gate pass. Edit the root-owned `/etc/panopticon/panopticon.env` (0640, group panopticon) to the actual paths.
5. Run the read-only preflight as the service user with the environment loaded. It checks runtime dependencies, repository/binary/env ownership, source reviews, and Caddy syntax without fetching anything:

```console
sudo -u panopticon bash -c 'set -a; source /etc/panopticon/panopticon.env; set +a; exec /srv/panopticon/repo/deploy/selfhost/preflight.sh'
sudo systemd-analyze verify /etc/systemd/system/panopticon-*.service /etc/systemd/system/panopticon-*.timer
```

Preflight failure must be resolved before enabling collection. The twelve recorded source-policy reviews expire December 9, 2026 at 22:30:23 UTC. Confirm the machine clock/NTP first; source freshness cannot be established from a wrong clock.

## Sandbox and release checks

Run the real Bubblewrap/Poppler/Tesseract integration test and `python3 scripts/test-docx-sandbox.py` under the actual service account and comparable service isolation before fetching documents. Build `cargo test --locked -p pnull-ingest --test real_sandbox_pdf --no-run`, locate the executable in `target/debug/deps`, and invoke it with `--ignored --nocapture`. Repeat inside a transient systemd service with the deployed unit's `NoNewPrivileges`, capability, filesystem, and temporary-mount restrictions. Ubuntu may restrict unprivileged user namespaces through AppArmor: resolve an actual denial with a narrow Bubblewrap profile rather than removing the host-wide restriction.

The collector, candidate, and publisher units mount bounded private temporary filesystems: `/tmp` is 512 MiB with 32,768 inodes; `/var/tmp` is 32 MiB with 4,096 inodes. `TMPDIR=/tmp` confines aggregate extractor workfiles to that mount. These replace `PrivateTmp` for those units and need host verification. systemd documents `TemporaryFileSystem` and mount options in [systemd.exec](https://www.freedesktop.org/software/systemd/man/latest/systemd.exec.html). The publisher additionally has no network namespace connectivity; the collectors can reach HTTPS sources but can write only private state and sanitized health. Host kernel/sandbox escapes, infrastructure denial of service, provider compromise, and private evidence-store growth remain operational risks. Keep disk/inode alerts and consider a quota for the evidence volume.

Start `panopticon-publish.service` first and inspect its journal/status. Check that `/srv/panopticon/site/current` resolves below the release root, all five cases and their local portraits load, failed votes stay labeled failed, and `/api/v1/alerts.json`, `/api/v1/feed.json`, `/api/v1/coverage.json`, and `/api/v1/research.json` parse correctly. Confirm private preview, evidence, `.git`, environment and database paths return 404. A dropped case must have an explicit withdrawal; the prior API history prevents silently erasing it.

After the release works locally, install the validated proposed Caddy config as the active config, point Namecheap's apex A record at the actual VPS IP, remove conflicting parking/redirect records, and verify externally valid TLS plus HTTP-to-HTTPS redirect. Add AAAA only after working IPv6 is verified. The server's hostname must match `PNUL_SITE_URL=https://panopticon.fail` and case metadata. The Caddy public roots are the release symlink and the sanitized status directory only; no reverse proxy to the private collector is required.

Then start the capture chain and activate schedules:

```console
sudo systemctl start panopticon-collect.service
sudo systemctl status panopticon-collect.service panopticon-candidates.service panopticon-publish.service
sudo systemctl enable --now panopticon-collect.timer panopticon-research.timer panopticon-health.timer
systemctl list-timers 'panopticon-*'
```

The standalone candidate timer is disabled by default because collection completion already invokes it. The empty candidate matrix reports disabled. A known source review or endpoint failure reports degraded even if a valid release remains available. Configure an external monitor for health state, health artifact age (at most two hours), TLS expiry, disk/inode usage, source review expiry, and failed systemd jobs. No outbound notification recipient is configured yet. A future X bot must stop on stale/degraded health, use `revision_id` deduplication, distinguish historical decisions from new events, and handle corrections/withdrawals. The API's generation time is not a fresh evidence capture timestamp. Colorado Springs remains static until its legacy sources receive capture reviews.

## Encrypted backup and restore

Install restic, initialize a separate off-server repository, and place the password in a restricted root-managed file readable by `panopticon` (for example `/etc/panopticon/restic-password`, 0640). Put `RESTIC_REPOSITORY` and `RESTIC_PASSWORD_FILE` in `/etc/panopticon/backup.env`, owned by root and group panopticon with mode 0640. Keep storage credentials and a password recovery copy outside this VPS. Use a dedicated backup account with limited access; configure immutable storage or separate retention credentials where the backend supports it. Scheme validation in `backup.sh` cannot prove the target is off-host or immutable.

Start `panopticon-backup.service`, check its exit status and `restic snapshots`, then restore a snapshot to an isolated directory on another machine. The backup contains coherent SQLite copies under its temporary `ops/.backup.<id>/pnull.db` and `ops/.backup.<id>/research.sqlite3` archive paths, the original private evidence/records/discovery/research directories at their original absolute paths, configured private proposal inputs, and the current public release. Reassemble those into a fresh private state directory. For research, restore the staged SQLite copy as `research/research.sqlite3` without replaying the separately archived old `research.sqlite3-wal` or `research.sqlite3-shm` files. Do not overwrite the live database during a test. Run `PRAGMA integrity_check` on both databases, verify archived blob digests through a production export, and compare the restored public API revision IDs with the saved release. Copy root-owned deployment env/config and the pinned code revision from their separate encrypted recovery copies. Enable `panopticon-backup.timer` only after this restore succeeds. Backup credentials are not required for serving the static website.

## Rollback, expiry, and incident handling

Publication always stages and validates before replacing `current`; failures keep the last good release and update health. Keep prior releases until the restoration drill is complete. Roll back with an atomic symlink replacement to a known validated release while collectors are stopped, and preserve current alert API withdrawal history so a rollback does not revive withdrawn claims. When rolling back application code, regenerate the release with the current verified data/history where possible rather than restoring old public assertions.

A source review expiry stops that source; it is never extended automatically. A changed source digest blocks facts that rely on it until the exact revision is reconciled or a correction/withdrawal is issued. For an active incident, disable collection and publication timers, preserve logs and evidence privately, revoke affected credentials, keep a known validated static release or a maintenance response, and rebuild the host from a clean image if host integrity is uncertain. No X posting credentials belong on this initial production server.
