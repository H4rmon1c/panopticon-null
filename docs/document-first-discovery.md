# Private document-first discovery

The scheduled Colorado collectors now start from five official Legistar **HTML meeting calendars** rather than the Legistar event API. Each calendar source is a fixed `Calendar.aspx` URL in its jurisdiction config. `pnull source discover-calendar <source-id>` uses the same source-review, DNS-safe HTTPS, conditional-request, interval, and byte-limit controls as `source archive`. It retains exact HTML bytes and fetch observations in the private store.

The command extracts same-host `MeetingDetail.aspx` links and `View.ashx` agenda, agenda-packet, and minutes links. It writes a private, digest-named candidate artifact at `discovery-candidates/<source-id>/<calendar-sha256>.json`. Each candidate records the link URL, kind, anchor position in the archived HTML, and source snapshot digest. Repeated captures of identical bytes produce the same artifact. A changed page gets a new artifact; a page with no eligible links fails instead of reporting a false empty calendar. The parser does not follow links, crawl arbitrary hosts, or create public facts.

```sh
pnull --config configs/states/co-commerce-city.toml \
  --data-dir /var/lib/panopticon source review verify commerce-city-calendar
pnull --config configs/states/co-commerce-city.toml \
  --data-dir /var/lib/panopticon source discover-calendar commerce-city-calendar
```

The first command will fail until a current review of that exact calendar endpoint is recorded. Do not enable unattended retrieval merely because the endpoint is public. The review is at the **source level**, not for every new record. The scheduled matrix checks the review before each fetch. The candidate artifact and the raw source archive stay outside the static web root.

This is a discovery queue, not complete coverage. The default calendar view may omit older meetings or documents. A candidate agenda, minute, contract, or vote must be fetched under its own reviewed source policy, archived byte-for-byte, parsed with exact locators, and checked against corroborating records before a case can be published. Where a published HTML/PDF record does not expose an individual roll call, a bounded official API response can still supply that narrow fact. The public site never calls that API.

Next, add a reviewed same-host path policy for candidate documents (with explicit host and path boundaries, rate limits, revision checks, and robots/terms review) so new links can be archived automatically. Build a candidate-to-case processor that distinguishes direct votes from consent blocks, failed motions from approvals, authorization from contract execution, and gaps from established links. Keep corrections and explicit rejections as publication vetoes.
