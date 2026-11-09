# Chino Axon agreement approval — preserved source responses

These are the exact response bytes retrieved from official Chino sources on 2026-11-09 UTC. Verify them with `sha256sum -c SHA256SUMS` from this directory. The case is a narrow first approval chain, not a claim about every Axon purchase or deployment in Chino.

| File | Official source URL | Relevant fields |
|---|---|---|
| `chino-matter-26-308.json` | `https://webapi.legistar.com/v1/chino/Matters/3139` | Matter 26-308, agreement description, amount, status |
| `chino-event-1961.json` | `https://webapi.legistar.com/v1/chino/Events/1961` | June 16, 2026 City Council meeting |
| `chino-event-item-52243.json` | `https://webapi.legistar.com/v1/chino/Events/1961/EventItems/52243?AgendaNote=1&MinutesNote=1&Attachments=1` | Axon matter as consent item 9, marked approved |
| `chino-consent-item-53661.json` | `https://webapi.legistar.com/v1/chino/Events/1961/EventItems/53661` | Motion approving Consent Calendar items 1–14 |
| `chino-votes-53661.json` | `https://webapi.legistar.com/v1/chino/EventItems/53661/Votes` | Five recorded AYES votes on that motion |
| `chino-person-{264,260,267,239,221}.json` | `https://webapi.legistar.com/v1/chino/Persons/<id>` | Exact Legistar person ID, name, and public office email for each voter |
| `chino-office-ulloa.html` | `https://cityofchino.org/m/directory/employee?eid=40` | Mayor; 2024 to 2028 |
| `chino-office-burton.html` | `https://cityofchino.org/580/Mayor-Pro-Tem-Curtis-Burton` | Mayor Pro Tem, District 2; 2022–2026 |
| `chino-office-comstock.html` | `https://cityofchino.org/518/Karen-Comstock` | Council Member, District 4; 2024 to 2028 |
| `chino-office-flores.html` | `https://cityofchino.org/519/Christopher-Flores` | Council Member, District 1; 2024 to 2028 |
| `chino-office-lucio.html` | `https://cityofchino.org/517/Marc-Lucio` | Council Member, District 3; 2022 to 2026 |

The City Council's motion approved Consent Calendar items 1–14. The Axon agreement is item 9, marked approved. The five individual votes belong to the **consent-calendar motion**, not to a separate roll call on item 9. The matter describes authorization of a five-year Axon Basic with Full AI agreement for $4,464,008.92. It does not establish an executed contract, payment, deployment, or exact start and end dates within the published term years. Those remain explicit gaps.

The importer matches each Legistar person ID and name to the vote, then the public office email to the City's office profile. This handles the difference between “Karen C. Comstock” in Legistar and “Karen Comstock” on the City page without relying on name similarity. The saved HTML may contain unrelated public contact details; the importer uses only the public office title, term-year span, and public office email for the identity join. All proposed links start **Pending**. A human must inspect the preserved bytes, locators, and quotations, then approve each binding before it appears in a public chain. This fixture is not itself a public website export.
