var client;
const $ = (selector, root = document) => root.querySelector(selector);
const $$ = (selector, root = document) => [...root.querySelectorAll(selector)];

const TYPE_GROUPS = {
  FACILITY: new Set(["FACILITY", "DATACENTER", "CAMPUS", "PROJECT"]),
  ORGANIZATION: new Set(["ORGANIZATION", "COMPANY", "AGENCY", "OPERATOR"]),
  UTILITY: new Set(["UTILITY", "POWER", "SUBSTATION"]),
  NETWORK: new Set(["NETWORK", "CARRIER", "FIBER"]),
  CONTRACTOR: new Set(["CONTRACTOR", "SUPPLIER", "VENDOR"]),
  CONTRACT: new Set(["CONTRACT", "AGREEMENT", "AWARD", "SOLICITATION"]),
};

const TYPE_STYLE = {
  FACILITY: ["#64e7ff", "FAC", "square"],
  ORGANIZATION: ["#be92ff", "ORG", "circle"],
  UTILITY: ["#ffbd37", "PWR", "diamond"],
  NETWORK: ["#78a7ff", "NET", "circle"],
  CONTRACTOR: ["#8fffb1", "BLD", "triangle"],
  SUPPLIER: ["#8fffb1", "SUP", "triangle"],
  CONTRACT: ["#ff5470", "CTR", "document"],
};

const GEO = {
  ent_northstar: { lat: 47.6062, lon: -122.3321, label: "Seattle demonstration headquarters" },
  ent_front_range: { lat: 38.8339, lon: -104.8214, label: "Colorado Springs demonstration region" },
  ent_meridian: { lat: 39.7392, lon: -104.9903, label: "Denver demonstration region" },
  ent_orion: { lat: 33.4484, lon: -112.074, label: "Phoenix demonstration headquarters" },
  ent_vectorlink: { lat: 32.7767, lon: -96.797, label: "Dallas demonstration network office" },
  ent_arc_tensor: { lat: 37.3382, lon: -121.8863, label: "San Jose demonstration headquarters" },
  ent_power_agreement: { lat: 38.94, lon: -104.7, label: "Front Range agreement anchor" },
};

const WORLD = [
  [[-168,71],[-150,72],[-137,66],[-128,59],[-124,50],[-124,41],[-117,32],[-107,24],[-97,18],[-88,16],[-83,9],[-77,8],[-79,24],[-75,35],[-66,45],[-54,47],[-58,55],[-72,60],[-92,74],[-118,73],[-140,69],[-168,71]],
  [[-82,13],[-75,9],[-66,8],[-54,3],[-47,-7],[-36,-8],[-39,-20],[-48,-28],[-53,-34],[-60,-43],[-67,-55],[-73,-50],[-75,-38],[-80,-25],[-82,-8],[-79,2],[-82,13]],
  [[-18,36],[-9,36],[0,37],[10,34],[20,32],[31,31],[42,12],[51,11],[50,1],[42,-12],[35,-25],[28,-34],[18,-35],[10,-29],[2,-18],[-7,4],[-16,14],[-18,36]],
  [[-11,36],[-10,44],[-3,51],[8,55],[20,57],[30,63],[42,66],[54,63],[67,70],[88,74],[111,71],[137,60],[157,59],[174,51],[164,43],[147,38],[132,34],[121,23],[108,20],[100,8],[91,9],[80,20],[68,23],[57,27],[44,30],[34,36],[25,40],[14,45],[3,43],[-11,36]],
  [[112,-10],[125,-10],[137,-14],[153,-25],[151,-37],[139,-43],[124,-35],[115,-26],[112,-10]],
  [[-54,83],[-30,82],[-18,72],[-28,61],[-46,59],[-60,66],[-54,83]],
  [[43,-13],[51,-16],[50,-25],[44,-25],[43,-13]],
  [[130,31],[142,35],[145,44],[140,47],[132,40],[130,31]],
];

const state = {
  mode: "connecting",
  status: null,
  activity: [],
  entities: [],
  byId: new Map(),
  sources: new Map(),
  selected: null,
  activeLayers: new Set(Object.keys(TYPE_GROUPS)),
  view: "global",
  sensor: "record",
  density: "tactical",
  activityPaused: false,
  activityCursor: 0,
  searchResults: [],
  searchCursor: 0,
  timelineEvents: [],
};

var globe;
var toastTimer;
var searchTimer;
var timelineTimer;
