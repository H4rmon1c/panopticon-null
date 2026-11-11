# Municipal website discovery leads

The [Colorado Municipal League membership directory](https://www.cml.org/home/networking-events/membership/membership) lists member municipalities and links to websites it knows about. A single HTML response was read on 2026-11-10 after checking its [robots file](https://www.cml.org/robots.txt). Its SHA-256 digest was `eb6e541a9e14b100db8b5d46fc628bbdfd9ff554a76858bb8f9e5cbc37e4f9ec`. The robots file allowed the membership path and specified a 120-second crawl delay; this work made no requests to the listed municipal websites.

[`co-municipal-websites.v1.json`](../configs/states/co-municipal-websites.v1.json) maps the 269 listed names to exact Census place IDs and retains each listing name, website lead, and locator. The source lists a URL for 241 entries and no URL for 28. Of the links, 142 use HTTP and 99 use HTTPS. Four of the 273 Census municipality entries are absent from this directory snapshot: Bonanza, Carbonate, Denver, and Hartman. The directory's membership count and the Census geography roster describe different things and are not interchangeable.

These are **directory leads**, not verified official document endpoints. An HTTP link must not be promoted into the HTTPS collector merely because it appears here. A listed website is not evidence of a surveillance decision, vote, contract, or portrait. The [offline worklist](colorado-worklist.md) exposes the directory status and URL while keeping directory-only jurisdictions at `not_started`. A source profile still needs an exact official documents page, a path policy, and a source-level access review before collection.

To refresh the directory, save one permitted HTML response and run:

```sh
python3 web/panopticon-terminal/scripts/build-colorado-municipal-directory.py \
  --input-html /path/to/cml-membership.html --archived-on YYYY-MM-DD \
  --output configs/states/co-municipal-websites.v1.json
```

The builder rejects unmatched or repeated names, unsafe link schemes, excessive input, and ambiguous directory markup. It does not crawl any municipal site. The original HTML was used for this one-time lead extraction and is not published as an evidence archive; the digest allows the extraction source to be identified. If a municipality is missing or a link has changed, verify its current official site separately before configuring a collector.
