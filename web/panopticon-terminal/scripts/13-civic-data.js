const civic$ = (selector, root = document) => root.querySelector(selector);
const civic$$ = (selector, root = document) => [...root.querySelectorAll(selector)];

const TYPE_META = {
  FACILITY: { color: "#66ddea", short: "PLACE", title: "Project or place" },
  DATACENTER: { color: "#66ddea", short: "SITE", title: "Datacenter" },
  CAMPUS: { color: "#66ddea", short: "SITE", title: "Campus" },
  PROJECT: { color: "#66ddea", short: "PROJ", title: "Project" },
  ORGANIZATION: { color: "#a892db", short: "ORG", title: "Organization" },
  COMPANY: { color: "#a892db", short: "CO", title: "Company" },
  AGENCY: { color: "#a892db", short: "AGCY", title: "Public agency" },
  JURISDICTION: { color: "#66ddea", short: "PLACE", title: "Jurisdiction" },
  TECHNOLOGY: { color: "#7fa7df", short: "TECH", title: "Technology" },
  OPERATOR: { color: "#a892db", short: "OPER", title: "Operator" },
  UTILITY: { color: "#f3b84b", short: "PWR", title: "Utility" },
  POWER: { color: "#f3b84b", short: "PWR", title: "Power record" },
  SUBSTATION: { color: "#f3b84b", short: "GRID", title: "Grid asset" },
  NETWORK: { color: "#7fa7df", short: "NET", title: "Network" },
  CARRIER: { color: "#7fa7df", short: "NET", title: "Carrier" },
  FIBER: { color: "#7fa7df", short: "NET", title: "Fiber" },
  CONTRACTOR: { color: "#82d5a0", short: "BLDR", title: "Builder" },
  SUPPLIER: { color: "#82d5a0", short: "SUP", title: "Supplier" },
  VENDOR: { color: "#82d5a0", short: "VEND", title: "Vendor" },
  CONTRACT: { color: "#e36d7d", short: "MONEY", title: "Contract" },
  AGREEMENT: { color: "#e36d7d", short: "AGR", title: "Agreement" },
  AWARD: { color: "#e36d7d", short: "AWARD", title: "Award" },
  SOLICITATION: { color: "#e36d7d", short: "BID", title: "Solicitation" },
  DECISION: { color: "#f0d579", short: "DEC", title: "Decision" },
  VOTE: { color: "#f0d579", short: "VOTE", title: "Vote" },
  ORDINANCE: { color: "#f0d579", short: "LAW", title: "Ordinance" },
  PERMIT: { color: "#f0d579", short: "PERM", title: "Permit" },
  PERSON: { color: "#d8e0e2", short: "ROLE", title: "Person in public role" },
  OFFICIAL: { color: "#d8e0e2", short: "ROLE", title: "Public official" },
  OFFICEHOLDER: { color: "#d8e0e2", short: "ROLE", title: "Officeholder" },
  RECORDED_VOTER: { color: "#d8e0e2", short: "VOTE", title: "Named voter in a public record" },
};

const STAKES = {
  FACILITY: "Projects like this can affect land use, utility capacity, tax incentives, traffic, water, and public infrastructure.",
  DATACENTER: "Datacenter projects can reshape grid planning, land use, water demand, tax policy, and local infrastructure spending.",
  CAMPUS: "Large campuses can reshape grid planning, land use, tax policy, traffic, water, and public infrastructure spending.",
  PROJECT: "Public and private projects can change a neighborhood long before most residents hear about them.",
  ORGANIZATION: "Organizations become understandable when ownership, contracts, projects, public filings, and counterparties are connected.",
  COMPANY: "Companies often touch public money, permits, infrastructure, and policy through records scattered across many systems.",
  AGENCY: "Agencies exercise public authority. Their decisions, contracts, meetings, and changes should be legible to the people affected.",
  JURISDICTION: "A jurisdiction sets the public authority and geographic scope for the decisions shown here.",
  TECHNOLOGY: "A technology is linked only where the public record identifies it in a decision, contract, or award.",
  UTILITY: "Utility records can reveal major load commitments, infrastructure costs, service agreements, and long-term local planning decisions.",
  NETWORK: "Network relationships can reveal who connects, serves, builds, or depends on critical infrastructure.",
  CONTRACTOR: "Contractors connect public decisions to the companies doing the work and receiving the money.",
  SUPPLIER: "Supplier relationships expose the procurement chain behind a project rather than stopping at the prime contractor.",
  CONTRACT: "Contracts show where money, obligations, and risk actually move. The details matter more than the announcement.",
  AGREEMENT: "Agreements turn plans into obligations. They show who committed to what, when, and under which public record.",
  DECISION: "Decisions matter most when the responsible body, vote, beneficiaries, costs, and downstream effects are visible together.",
  PERSON: "People are shown only in their documented public or organizational roles, never as private surveillance targets.",
  RECORDED_VOTER: "The public record names this voter on a specific decision. A public role and term require separate evidence.",
};

const ACTIVITY_LABELS = {
  RELATION_ADDED: "NEW CONNECTION",
  SOURCE_CHANGED: "SOURCE CHANGED",
  PROJECT_OBSERVED: "PROJECT UPDATE",
  ENTITY_UPDATED: "RECORD UPDATED",
  CONTRACT_OBSERVED: "CONTRACT FOUND",
  SOURCE_ADDED: "NEW SOURCE",
  EVIDENCE_REVISED: "PROOF REVISED",
  ENTITY_ADDED: "NEW RECORD",
};

let atlas = null;
let audience = "public";
let place = "COLORADO SPRINGS, CO";

function currentState() { return state; }
