# Additional Colorado calendar sources — 2026-11-10

Two current official meeting calendars fit the bounded `discover-calendar` HTML parser. They add **source reach**, not investigated decisions or claims about surveillance purchases. Both need current source-level robots and terms review before unattended capture. Their meeting-page and document paths are separately configured for one-candidate-at-a-time archiving after path review; those path policies are not scheduled.

| Jurisdiction and atlas geography | Official public link | Configured source | Current parser check |
| --- | --- | --- | --- |
| Douglas County, `us-co-county-08035` | [County meetings platform](https://www.douglasco.gov/board-county-commissioners/boards-commissions/) → [Legistar calendar](https://douglascounty.legistar.com/Calendar.aspx) | `douglas-county-calendar` | Calendar HTML exposed 19 meeting-detail links and agenda/packet links in a bounded November 2026 check. |
| Littleton city, `us-co-place-0845255` | [City meeting guide](https://www.littletonco.gov/Government/Littleton-Leadership/Meeting-Videos-Agendas) and [current city-branded Legistar calendar](https://littletongov.legistar.com/Calendar.aspx) | `littleton-calendar` | Calendar HTML exposed four meeting-detail links and agenda/packet links in a bounded November 2026 check. |

The exact source-to-geography bindings are in [`co-calendar-geography.v1.json`](../configs/states/co-calendar-geography.v1.json); `test-calendar-profiles.py` checks all seven scheduled calendars and `test-candidate-profiles.py` checks all fourteen candidate path policies. A calendar page generally shows only the current view; it is not historical or jurisdiction-wide case coverage. Other agenda platforms require their own documented adapters before scheduling.
