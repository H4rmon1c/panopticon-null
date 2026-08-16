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

`pdf/scanned-surveillance-text.pdf` is a test-only, image-only derivative of page 1 of the preserved official presentation, rasterized at 80 DPI. Its SHA-256 is `77779970241d1478db90c7a6d3eac51ba9a7c3c2b3ce17c5ef77bf1bf28cf544`; it exists only to exercise optional OCR and is never represented as an original government file.

Files under `html/` and `hostile/` are synthetic parser-security test inputs. They are not represented as government records.
