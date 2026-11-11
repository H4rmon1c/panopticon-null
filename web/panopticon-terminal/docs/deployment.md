# PANOPTICON.FAIL static deployment

The first release serves a sanitized static dataset. The browser never connects to the collector or its private database.

```text
internet
   |
Caddy / nginx
   `-- static Civic Intelligence Atlas
       `-- public/public-snapshot.json
                    ^
             gated Rust atlas export
                    ^
             private collector state
```

The checked-in public snapshot contains the approved Colorado Springs case. Pending future work is exported only to the gitignored `preview-snapshot.json` for owner-only editorial QA. Do not copy that file into a public deployment.

## Release sequence

From the Rust repository root, run `atlas requirements`, verify source bindings, complete one case-level release decision, then `atlas allow-release`. Set the canonical site URL explicitly and export to the static site:

```console
cargo run -p pnull-cli -- --site-url "$PNUL_SITE_URL" atlas export --output /absolute/path/to/web/panopticon-terminal/public/public-snapshot.json
node web/panopticon-terminal/scripts/validate.mjs
node --test web/panopticon-terminal/scripts/test-client.mjs
```

The release exporter must reject missing source verification and a pending or mismatched case release. Deploy the validated site and snapshot together. A failed export leaves the previous public dataset in place.

For owner-only editorial QA, the Rust CLI can write `preview-snapshot.json` with `atlas export --preview --output ...`; serve it only in a private site and open `?preview=1`. `?demo=1` selects the separate synthetic fixture. Neither mode is an automatic fallback.

## Host isolation

Use separate Unix users for the collector and static web server. The web server can read the site directory but cannot read collector credentials, operator notes, private database, or review workspace. `preview-snapshot.json` should be blocked or absent on public hosting. See [`../deploy/Caddyfile.example`](../deploy/Caddyfile.example).

Jurisdiction and agency points are coarse map anchors, not precise sites. The relationship canvas is schematic. The browser does not request device location. People are shown only in documented public roles.

A read-only API may follow when the released dataset grows. It must read only a sanitized public artifact and retain the same source-verification and case-release gates; the draft interface is in [`api-contract.md`](api-contract.md).
