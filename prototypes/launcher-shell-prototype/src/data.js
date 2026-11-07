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
    // #22 trial: textless 30th Anniversary key art (news.blizzard.com), see docs/research/d2r-forever-hero-image.md.
    art: CDN + 'blt286175c11a6b3f4c/bltbc24705a63d11582/6aee1babc978be3136a3cfe5/DIA_D2R_30th_Anniversary_Key_Art_1920x1080_nologo_GG01.png',
    artPos: '20% 30%',
    credit: '30th Anniversary key art',
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


// E/F Patch Notes tab: release notes per product, newest first (illustrative copy).
// News items tagged Release notes are dropped from the feed in E (#20 Q13).
export const NOTES = {
  pd2r: [
    { version: 'alpha 0.4.2', date: '3 days ago', summary: 'Loot filter fixes, corruption lines on items and a stash crash fix.',
      sections: [
        ['Loot filter', ['Rune names highlight again in the default filter.', 'Custom filters keep their colours after an update.', 'Fixed set items showing as uniques in some filters.']],
        ['Items', ['Corrupted items now show their corruption line in the tooltip.', 'Map item tooltips list all their modifiers.']],
        ['Fixes', ['Fixed a crash when opening the stash with a full cube.', 'Fixed mercenary equipment resetting after a zone change.', 'Fixed the Act 3 waypoint sometimes not registering.']],
      ] },
    { version: 'alpha 0.4.1', date: '3 weeks ago', summary: 'TCP/IP co-op join fixes and early Act 1 drop rates.',
      sections: [
        ['Co-op', ['TCP/IP games now list correctly on the join screen.', 'Players joining mid-game keep their party.']],
        ['Balance', ['Rebalanced early Act 1 drop rates.', 'Normal Countess drops runes more often.']],
      ] },
  ],
  forever: [],
  '09': [],
};
