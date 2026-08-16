# Fixture provenance

The Colorado fixtures are exact bytes retrieved from official public endpoints on 2026-08-16 with one request at a time and no authentication, access-control bypass, or browser automation.

| File | Official source | Purpose |
|---|---|---|
| `co/ordinance-25-93-draft.pdf` | [Legistar attachment 14876734](https://coloradosprings.legistar.com/View.ashx?M=F&ID=14876734&GUID=754550C8-73DC-49BC-B779-9ECDAB97D404) | Original ordinance text |
| `co/ordinance-25-93-signed.pdf` | [Legistar attachment 14995655](https://coloradosprings.legistar.com/View.ashx?M=F&ID=14995655&GUID=00173B3A-1CD9-4839-9AD5-93B3C8E4EE03) | Final signed ordinance and passage dates |
| `co/police-technology-surcharge-presentation.pdf` | [Legistar attachment 14876735](https://coloradosprings.legistar.com/View.ashx?M=F&ID=14876735&GUID=EA1466B1-D5D6-48CF-BB70-7710853E363D) | Supporting source naming Axon body cameras, Axon evidence systems, and Flock vehicle-intelligence cameras with stated costs |
| `co/event-2654-work-session.json` | [Legistar event 2654](https://webapi.legistar.com/v1/coloradosprings/events/2654?EventItems=1&AgendaNote=1&MinutesNote=1&EventItemAttachments=1) | Real API parser and work-session fixture |
| `co/event-2660-final-vote.json` | [Legistar event 2660](https://webapi.legistar.com/v1/coloradosprings/events/2660?EventItems=1&AgendaNote=1&MinutesNote=1&EventItemAttachments=1) | Real API parser and final-vote fixture |

`co/SHA256SUMS` commits the hash of every source fixture. Raw fixtures remain local evidence inputs; the static publisher emits only necessary institutional facts and exact selected citations.

## Second genuine matter (v0.0.2)

`co2/` preserves the second genuine Colorado Springs matter: **Ordinance No. 15-84 (2015)**, which established the municipal court Information Technology Surcharge that the v0.0.1 matter (Ordinance 25-93) amends. Bytes were retrieved 2026-08-16 from the reviewed Legistar API, one request at a time, no authentication.

| File | Official source | Purpose |
|---|---|---|
| `co2/matter-15-00663-ordinance-15-84.json` | [Legistar matter 2971](https://webapi.legistar.com/v1/coloradosprings/matters/2971) | Matter record identifying Ordinance 15-84 (the subject) |
| `co2/event-1109-2015-11-24.json` | [Legistar event 1109](https://webapi.legistar.com/v1/coloradosprings/events/1109?EventItems=1&EventItemAttachments=1) | City Council meeting that finally passed Ordinance 15-84 (the institutional action) |
| `co2/event-1104-2015-10-12-bwc.json` | [Legistar event 1104](https://webapi.legistar.com/v1/coloradosprings/events/1104/EventItems?EventItemAttachments=1) | Meeting receiving the 2015 Body Worn Camera Project Summary (informational) |

`co2/SHA256SUMS` commits the hash of each second-matter fixture.

The surveillance-technology link is documented, not assumed: the preserved 2025 presentation (`co/police-technology-surcharge-presentation.pdf`) states the surcharge funds Axon body cameras, Axon digital evidence systems, Axon AI transcription, and Flock vehicle-intelligence cameras. The 2015 matter record and event establish that the surcharge was adopted; the 2025 presentation (a supporting document) connects that surcharge to surveillance technology. No contract, purchase order, or award for a specific vendor is preserved or asserted.

`pdf/scanned-surveillance-text.pdf` is a test-only, image-only derivative of page 1 of the preserved official presentation, rasterized at 80 DPI. Its SHA-256 is `77779970241d1478db90c7a6d3eac51ba9a7c3c2b3ce17c5ef77bf1bf28cf544`; it exists only to exercise optional OCR and is never represented as an original government file.

Files under `html/` and `hostile/` are synthetic parser-security test inputs. They are not represented as government records.
