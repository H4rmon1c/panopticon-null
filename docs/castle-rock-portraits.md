# Castle Rock official portrait set

The seven direct recorded voters for Resolution 2025-071 each have an exact-name [Town of Castle Rock council directory profile](https://www.crgov.com/2270/Meet-Council) with an individual photograph. The vote source identifies each person by `VotePersonId`; a separate official Legistar `Persons/{id}` response supplies the same full name and town email address. The directory's matching email address links the two systems. A town profile does **not** itself contain the Legistar ID.

| Recorded voter | Official directory profile | Official person record | Profile image locator |
| --- | --- | --- | --- |
| Jason Gray | [profile](https://www.crgov.com/m/directory/employee?eid=22) | [419](https://webapi.legistar.com/v1/castlerock-co/Persons/419) | `img[alt="Jason Gray"]` |
| Ryan Hollingshead | [profile](https://www.crgov.com/m/directory/employee?eid=276) | [492](https://webapi.legistar.com/v1/castlerock-co/Persons/492) | `img[alt="Ryan Hollingshead"]` |
| Kevin Bracken | [profile](https://www.crgov.com/m/directory/employee?eid=18) | [420](https://webapi.legistar.com/v1/castlerock-co/Persons/420) | `img[alt="Kevin Bracken"]` |
| Mark Davis | [profile](https://www.crgov.com/m/directory/employee?eid=278) | [626](https://webapi.legistar.com/v1/castlerock-co/Persons/626) | `img[alt="Headshot photo of District 4 Councilmember Mark Davis"]` |
| Max Brooks | [profile](https://www.crgov.com/m/directory/employee?eid=20) | [342](https://webapi.legistar.com/v1/castlerock-co/Persons/342) | `img[alt="Max Brooks"]` |
| Tim Dietz | [profile](https://www.crgov.com/m/directory/employee?eid=21) | [494](https://webapi.legistar.com/v1/castlerock-co/Persons/494) | `img[alt="Tim Dietz"]` |
| Laura Cavey | [profile](https://www.crgov.com/m/directory/employee?eid=277) | [493](https://webapi.legistar.com/v1/castlerock-co/Persons/493) | `img[alt="Councilwoman Cavey"]` |

The town's Cloudflare service allowed normal browser access but returned HTTP 403 for direct HTML downloads. The source fixtures therefore preserve **browser DOM extracts**, not raw HTML responses. Each extract records the observed profile heading, photograph URL and alt text, and contact link, with a SHA-256 digest and retrieval date of 2026-11-10. The source image responses were downloaded through the browser asset inventory without resizing and stored as local JPEGs with pinned SHA-256 digests. The official Person JSON responses were preserved byte for byte. The release validator checks all three evidence parts and the exact town, portal client, person ID, name, and email bridge. The public vote card shows the preserved image and official source link; it does not show email.

**Unresolved source conflict:** Tim Dietz's official Legistar Person record has `PersonWWW` ending `eid=20`, which is the town directory entry used by Max Brooks. Tim's own town profile is `eid=21` and matches the Person record's name and town email. The portrait registry pins the conflicting `PersonWWW` value, and the vote card displays a source-conflict note next to Tim's photo. This does not establish why the portal link is wrong. Recheck the town and portal records before releasing the Castle Rock case.

These current directory portraits do not establish what a voter looked like on the June 2025 decision date. Their capture date is known; the portrait date remains **not established**. The saved JPEGs are complete bytes served by the town's image endpoint, without a claim that they are the photographer's original files.
