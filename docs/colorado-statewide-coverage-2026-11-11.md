# Colorado statewide research snapshot — November 11, 2026

This is a dated collection result, not a claim that Colorado has been
completely searched. The public atlas has five evidence-backed decisions in
four municipalities. The separate preliminary research snapshot contains
274 source references in 55 jurisdictions, including 39 references in 12
provisional special districts. It records 78 actions and 491 vote entries;
these include failed motions, negative votes, and votes on consent blocks.
A vote on a consent block is not represented as a separate item vote.

The research display can associate sourced portraits with 68 of the 71
distinct voters in that snapshot. Matching requires the relevant jurisdiction,
source-specific identity and name to agree. A portrait does not establish a
person's term, their appearance on the vote date, a contract, or deployment.

## Collection reached so far

- Across 64 counties and 273 municipalities, 309 directory website leads
  were attempted: 153 latest checks reached a site and 156 were unavailable
  under the collector's rules. Twenty-eight municipalities had no usable
  directory website lead.
- For 178 mapped school districts, 177 had website leads and were attempted;
  153 latest checks reached a site and 24 were unavailable. One school
  district had no bound website lead.
- For 3,595 provisional special-district entries, 1,094 unique-root website
  leads were attempted. On 661 sites the district name was established; 433
  sites were unavailable or the name was not established. The other 2,501
  entries had no attempted lead under this adapter. Many share a host or use
  a path-scoped tenant and need a source-specific attribution method.
- The snapshot includes 25,039 completed document keyword screens, 2,594
  incomplete attempts, and 1,618 source redirects with linked alternate
  documents. At least one document or portal page was screened in 545
  jurisdictions. These are bounded searches, not a complete date window.

A reached website does not mean that its full meeting history was found or
that a missing reference proves no relevant decision exists. Preliminary
references are source leads, not assertions that a contract was approved or
that an individual voted for it. School records omit names and excerpts until
an appropriate public-official identity is separately established.

## Updating and deployment

The self-host research timer schedules bounded discovery, screening, portrait
capture, and snapshot publication six hours after the preceding run finishes.
It has not been installed
or run on the owner's VPS. Raw response bytes, extraction results, network
receipts, and the research database stay in the private archive; the public
site receives sanitized research records, source links, locators, and digests.
Source links point to live websites and may change after capture.

The static `/api/v1/research.json` contains the preliminary research snapshot.
The separate `/api/v1/alerts.json` covers released decisions and withdrawal
notices; it is not an automatic alert for every preliminary reference. Neither
API posts to X. Deployments need the private archive and a tested backup in
addition to the Git checkout; the archive is not in the repository.

Next coverage work should address unavailable municipal sites, shared and
path-scoped district hosts, additional agenda platforms and historical date
windows, and unbound voter portraits. State agencies, universities, tribal
governments and other public bodies also need sourced discovery registries.
