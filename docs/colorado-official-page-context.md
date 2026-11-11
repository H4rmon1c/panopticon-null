# Official technology-page context

Five explicitly scoped municipal police pages provide technology context for
Broomfield (Flock ALPR), Boulder (police unmanned-aircraft team), and Westminster
(Axon body-worn cameras), as well as Greeley (Flock cameras) and Boulder (selection of Flock Safety).
These records are preliminary agency statements;
they do not establish a decision, an individual vote, a contract award or
execution, or independent verification that a deployment remains current.
Capture time and exact HTML text-node locators are included. The live source
may change after capture.

`python3 scripts/colorado-research.py --data-dir /absolute/private/research official-context`
uses the existing bounded HTTPS and robots handling. It requires each fixed URL
to match its exact known municipality's discovered host or a separately
verified canonical migration. The archived HTTP receipt, public-IP checks,
content digest, exact agency text and unique technology passage are rechecked
at export. A pinned normalized passage sequence binds each agency and claim,
including the intervening context; unrelated page changes can still pass.
Hidden markup is excluded; Westminster's exact named, initially
collapsed police transparency tab is the sole explicit tab-panel exception.
If a previously published source no longer matches the pinned contextual
passages and structural checks, the same reference ID becomes a publicly visible
withdrawal marker. The marker does not claim the previous statement was false.
It no longer states the prior technology claim or counts the jurisdiction as a
current mention. A changed or withdrawn successful capture retains its prior
receipt in `official_context_history`; raw bytes remain in the private content
store. If the source again satisfies the exact checks, the same ID is restored.
An unavailable fetch retains the last dated capture while the research service
returns a failure state. The self-host research schedule invokes this command.

This type is deliberately not a general-purpose HTML importer. Do not promote
EFF discovery leads or other page text to this type without a new exact
jurisdiction/agency binding, source-specific parser, adversarial tests, and a
public API/UI definition. The source and technology connection does not fill
the decision, contract, signature, official or vote fields.
