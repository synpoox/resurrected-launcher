// PROTOTYPE data for #17. Display names from resurrected-web branding.json @155ecee (provisional).
export const UMBRELLA = 'Project Resurrected'; // the one swappable place for the umbrella label

const CDN = 'https://blz-contentstack-images.akamaized.net/v3/assets/';
// Hotlinked, not copied: official D2R promotional art (c) Blizzard Entertainment, never gameplay.
export const PRODUCTS = [
  {
    id: 'pd2r',
    name: 'Project Diablo 2: Resurrected',
    short: 'PD2R',
    tag: 'Playable alpha',
    headline: ['The game you know.', 'A world reborn.'],
    blurb: 'The Project Diablo 2 experience, rebuilt on Diablo II: Resurrected.',
    art: CDN + 'bltf408a0557f4e4998/blt43f9dec60857465c/698d134704090043d66ddb1e/DIA_D2R_Garden_GameplayTrailer_thumbnail_1920x1080_KEYART2_VM01.png?imdensity=1&imwidth=1920',
    artPos: '55% center',
    credit: 'Reign of the Warlock key art',
    playable: true,
    readiness: [
      ['Release', 'alpha 0.4.2 (latest)'],
      ['D2R', '3.3 found'],
      ['Saves', '6 characters'],
    ],
    plugins: '2 plugins on',
  },
  {
    id: 'forever',
    name: 'Diablo II: Resurrected Forever',
    short: 'Forever',
    tag: 'Coming soon',
    headline: ['A different future.', 'A resurrected world.'],
    blurb: 'Class changes, legacy items, league mechanics and a reworked endgame.',
    art: CDN + 'blt9c12f249ac15c7ec/blt06acc1f47b6156a8/6962c5865a1d2cc3f390a43b/necromancer_background_2600.webp',
    // Class strips are 2600x906 with the character at ~70% across: zoom in and pull them toward centre.
    artPos: '74% 22%',
    artSize: 'auto 135%',
    credit: 'Necromancer class art',
    playable: false,
  },
  {
    id: '09',
    name: 'Diablo II: Resurrected 09',
    short: '09',
    tag: 'Coming soon',
    headline: ['An earlier era.', 'A world revisited.'],
    blurb: 'Lord of Destruction 1.09d, ported to Diablo II: Resurrected 3.3.',
    // Owner, round 2: Sorceress instead of Paladin (diablo2.blizzard.com class art).
    art: CDN + 'blt9c12f249ac15c7ec/blt0d46a551b84f5c65/6962c6e16abeed2776871844/sorceress_background_2600.webp',
    artPos: '74% 22%',
    artSize: 'auto 135%',
    credit: 'Sorceress class art',
    playable: false,
  },
];

export const NEWS = {
  pd2r: [
    { tag: 'Release notes', title: 'Alpha 0.4.2: loot filter fixes', date: '3 days ago' },
    { tag: 'Announcement', title: 'TCP/IP co-op testing weekend', date: '1 week ago' },
    { tag: 'Guide', title: 'Backing up your saves before an update', date: '2 weeks ago' },
  ],
  forever: [{ tag: 'Dev update', title: 'Legacy items: first look', date: '5 days ago' }],
  '09': [],
};

export const CREDIT = 'Official D2R promotional artwork © Blizzard Entertainment · Not mod gameplay';
