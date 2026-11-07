# Riot Games launcher reference: design evidence

Research for [#13](https://github.com/synpoox/resurrected-launcher/issues/13), part of the map [#12](https://github.com/synpoox/resurrected-launcher/issues/12).

**Source:** Juan Andrés Cutillas, [Riot Games | Game Launcher | UI/UX Design](https://www.behance.net/gallery/90336771/Riot-Games-Game-Launcher-UIUX-Design), Behance, published 7 January 2020. Project caption: "Riot Games - Game Launcher concept for upcoming games". Tools listed: Photoshop, Illustrator, Sketch. Licence shown: © All Rights Reserved. The closing board credits "all text and images" to Riot Games.

**Method:** viewed 2026-11-07 in the built-in browser. The project is eight static images with no captions, no video and no GIFs; every image's alt text is the same tag list, so the only designer text is what is printed inside the images (an "About" board and section titles). Each image was opened at its own Behance CDN URL and inspected at full size. Nothing was downloaded into this repository. Descriptions below are in words; image links go to the original CDN file.

**Prior evidence in this repo:** the [#8 resolution](https://github.com/synpoox/resurrected-launcher/issues/8) already used "the supplied Riot-style screenshot as a composition reference: dark product sidebar, large artwork hero, prominent install/Play action, readiness information, progress, settings/troubleshooting and news cards". That screenshot is not in the repo or the issue, so it cannot be compared here. Note the mismatch below: the Behance project shows the sidebar, hero, Play action and news cards, but **not** readiness, progress or settings.

## The boards

| # | Board (designer's title) | Link | What it shows |
|---|---|---|---|
| 1 | Cover | [image](https://mir-s3-cdn-cf.behance.net/project_modules/1400_webp/1f34cb90336771.5ed14a070ce1a.png) | Riot Games logo centred on a dark teal sky, four champions charging out of fire below. Corner labels "Game launcher" / "UI UX design". Presentation only. |
| 2 | Introduction: About | [image](https://mir-s3-cdn-cf.behance.net/project_modules/1400_webp/fd2f6290336771.5e14d7ce299f4.png) | Designer's rationale (summarised): after Riot announced new games in October 2019, he proposed a launcher in the vein of Battle.net, Epic and Origin that conveys Riot's "magical, playful, and innovative spirit". No UI. |
| 3 | Main screens structure; Main page: Home | [image](https://mir-s3-cdn-cf.behance.net/project_modules/1400_webp/e46d4290336771.5ed14a070c854.png) | A labelled wireframe, then the Home screen in a laptop mockup. |
| 4 | Games & projects (League of Legends, Teamfight Tactics) | [image](https://mir-s3-cdn-cf.behance.net/project_modules/1400_webp/0ca30d90336771.5eda2cf992c36.jpg) | Two per-game pages, each labelled with a vertical game name in the margin. |
| 5 | Games & projects, continued (Legends of Runeterra, Wild Rift, Projects) | [image](https://mir-s3-cdn-cf.behance.net/project_modules/1400_webp/6dcd4a90336771.5eda2cf993422.jpg) | Three more per-game pages, including an unreleased game and an "upcoming" project. |
| 6 | News | [image](https://mir-s3-cdn-cf.behance.net/project_modules/1400_webp/53619690336771.5ed14a070c31b.png) | End of the LoL page in a laptop, then a full news article view. Ends with the "About the game" heading. |
| 7 | About the game | [image](https://mir-s3-cdn-cf.behance.net/project_modules/1400_webp/2c67d190336771.5eda2cf991c6f.jpg) | The game page's second tab: description, release date, platforms, Play button. |
| 8 | Login & sign up; Thanks for watching | [image](https://mir-s3-cdn-cf.behance.net/project_modules/1400_webp/a1009190336771.5eda2cf992442.jpg) | Sign-in screen and two sign-up/recovery dialogs, then a closing champion group and credits. |

## Composition, module by module

### Layout skeleton (board 3 wireframe)

The designer's own wireframe names five regions: a full-height **Main menu** column on the left (about a sixth of the width), and to its right a **Main module** spanning the top ~45% of the content area, under which sit **two News modules** of equal width and a narrow right column stacked as **Quick access** over **Friends**. Every screen in the project follows this skeleton; only the content of the regions changes.

### Sidebar / product switcher (boards 3–7)

- A near-black column, darker than the content area. Logo (Riot fist + wordmark) at the top-left, then a plain text list: Home, League of Legends, Teamfight Tactics, Legends of Runeterra, LoL: Wild Rift, Projects.
- **Text only, no game icons.** The active item is white; inactive items are muted grey. No highlight bar, pill or background on the selected row is visible. Two Behance commenters (Feb 2021, Mar 2022) independently suggested adding an icon beside each game title to tell them apart at a glance.
- Items are generously spaced (roughly one item per 30 px at the shown 1400 px board width, much looser than a dense list).
- Pinned to the bottom of the column: **Adjustment** (gear icon, i.e. settings) and **Profile** (person icon).
- On boards 4–5 a red block bleeds out behind the left edge of each window, with the game name set vertically in the margin. That is presentation dressing for the board, not part of the launcher.

### Hero / main module

- Full-bleed key art filling the top of the content area, with the art also running up behind the area where cards begin, so cards sit on the darkened lower part of the image.
- Text block anchored top-left over the art: a small red **tag** (slanted parallelogram, uppercase, letter-spaced: e.g. "LEGENDS OF RUNETERRA", "CHAMPION REVEAL", "GAMEPLAY UPDATE", "NEW GAME"), a **bold two-line title**, a one- or two-line regular subtitle, then a call-to-action button.
- Art is pushed to the right half so the text has dark space on the left. On some boards a character breaks out of the frame (board 5: Wild Rift, the bear's claw and Annie overlap the window edge; board 4: the Teemo/Pengu mascots float outside). That is marketing composition and would not survive a fixed window.
- Home's hero promotes one item across games ("Pre-Register To Play Legends of Runeterra"); each game page's hero promotes that game's headline news.

### Primary action and its states

- Two button styles appear throughout:
  - **Solid red, rectangular, no radius**: "/ Get access" (Home hero), "/ PLAY FOR FREE" (About tab), "/ SIGN UP" (Wild Rift), the per-game **Play** button.
  - **Outlined (thin light border, transparent fill)**: secondary actions in the hero and on cards ("/ Watch now", "/ Learn more", "/ Sign up").
- Every button label starts with a slash ("/ Watch now") and is set in a monospaced-looking face, which is the project's most distinctive typographic signature.
- On game pages the **Play button sits at the top of the right column**, red, full column width, aligned with the top of the news row (board 4: "PLAY NOW" on League of Legends, partly covered by hero art, and "/ PLAY NOW" on TFT; board 5: what appears to be "/ PRE-REGISTER" on Runeterra; small text, read at board resolution).
- **No other states are shown.** There is no install, update, downloading, paused, repairing, running/"Playing", disabled or error state anywhere. There are no hover, focus or pressed variants. The launcher must invent all of these.

### Progress

**Not shown.** No progress bar, download speed, percentage, queue or patching indicator appears on any board.

### News cards and news article (boards 3–6)

- Cards are dark-grey panels (lighter than the sidebar, darker than nothing) with a **16:9-ish image on top**, then a small red category tag, a bold one-line headline, and an outlined "/ Learn more"-style button bottom-left. Square corners, no visible shadow inside the app.
- **Two cards per row** on Home and on each game page, filling the width left of the right column.
- On game pages a heading row above the cards reads **"News | About <game>"**: a two-tab switch where the active tab is white and the inactive one grey. Board 7 shows the About tab: game logo, "What is League of Legends?", two paragraphs, a red "/ PLAY FOR FREE" button, and a right-hand metadata column (Release date; Platform).
- Board 6 shows a **full article view** inside the same shell: breadcrumb at the top ("< League of Legends / Patch 9.24b notes"), large header art, red tag, title, byline with relative date ("By … · 8 days ago"), body text in the left two-thirds, and a "Table of contents" list on the right.
- Board 5's Wild Rift page replaces the cards with a white sign-up panel (two email fields, red "/ SIGN UP"). This is marketing capture.

### Upcoming products ("Projects")

The sidebar's last entry, **Projects**, opens a page for an unreleased game ("Project A", tagged "NEW GAME", "A stylish, competitive, character-based tactical shooter.", "/ Watch now") with the same hero + news + quick-access layout and **no Play button**. Wild Rift (pre-release at the time) also shows sign-up instead of Play. This is the closest analogue in the reference to our "Coming soon" products (Forever, 09).

### Settings

**Not shown.** "Adjustment" with a gear icon is in the sidebar footer, but no settings screen or panel was designed.

### Title bar / window chrome

**Not shown.** Every screen is placed in a laptop or floating-panel mockup with no title bar, drag region, or minimise/maximise/close controls visible. The logo sits flush at the top of the sidebar. A frameless custom title bar is our own decision (#12) with no evidence from this reference.

### Typography scale

Font names are not given anywhere in the project and cannot be identified reliably from the images. What can be seen:

- **Display/titles:** heavy, geometric sans (wide, bold), used for hero titles (two lines, largest size on screen) and for the designer's board titles in uppercase.
- **Card headlines:** the same family, bold, roughly half the hero title size, one line.
- **Body/subtitles/nav:** a regular-weight humanist or geometric sans, small, light grey on dark.
- **Tags:** tiny uppercase, wide letter-spacing, white on red parallelogram.
- **Buttons:** monospaced-looking, small, prefixed with "/".
- **Section labels** ("News", "Quick access", "Friends"): regular weight, small, white.

So roughly four steps: hero title, card title, body/nav, micro (tags/buttons). Exact sizes are not recoverable.

### Spacing density

Low density, generous margins: a wide sidebar with loose item spacing, a hero that takes almost half the window, only two news cards visible per screen, and a consistent gutter (visually about the same as the card padding) between cards and the right column. Content is aligned to a clear left edge shared by the hero text and the first card.

### Colour roles

| Role | As seen |
|---|---|
| App background / sidebar | near-black (sidebar slightly darker than content) |
| Surface (cards, About panel, article) | dark charcoal grey, flat |
| Primary accent | a single saturated **Riot red**: primary buttons, tags, Play, and the board backdrops |
| Text primary | white |
| Text secondary / inactive nav / inactive tab | mid grey |
| Secondary button | transparent with light-grey hairline border |
| Status (friends list only) | small green online dot on avatars |

Colour otherwise comes entirely from the key art. One accent colour carries every call to action.

### Motion cues

**Not shown.** The project is static images only (no video, GIF or prototype link). Nothing can be said about transitions, hover or loading motion from this source. The only "motion" impression is compositional: diagonal light streaks and characters breaking the frame in the hero art.

## Out of scope (social, store, profile, account)

These appear in the reference and stay out of our launcher, per #12:

- **Friends** module (avatars, name, current game or "Offline", online dot) in the right column.
- **Profile** entry in the sidebar footer.
- **Login & sign up** board: sign-in form beside a key-art panel, a sign-up dialog and a "can't sign in"/recovery dialog.
- **Email capture** panel (Wild Rift "When can I play?" sign-up).
- **Pre-register / Get access** marketing calls to action.
- **Quick access** tiles (logo tiles linking to other games) are cross-promotion; our sidebar already lists every product, so they add nothing.
- There is no store or marketplace screen in the reference.

## What to borrow

1. The **wireframe skeleton**: fixed left product column; one hero per product; a news row below; a narrow right column. Replace the Friends/Quick access column with our own content (for example readiness/install status and the Play action) rather than leaving it empty.
2. **Product-per-row text sidebar** with white active / grey inactive (whether we need a Home entry above three products is for the spec to decide), with **settings pinned to the bottom**. Consider adding the per-product mark the commenters asked for.
3. **Hero = key art pushed right, text block left**, with tag + bold title + one-line description + one action. Our art is official D2R promotional imagery, credited, so breakout characters do not apply.
4. **One accent colour for every call to action** on a near-black/charcoal palette; flat square surfaces.
5. **Red tag + title + outlined "learn more"** news card, two per row; optional full-article view with breadcrumb if the feed ever carries bodies (static feed + release-note links may make this unnecessary).
6. **"News | About" tab** on the product page as a place for product description, which suits Forever and 09 ("Coming soon"), mirroring the reference's Projects page that has no Play button.
7. **Slash-prefixed, monospaced button labels** are the reference's signature. Whether that suits a Diablo-themed launcher is a taste call for the owner.

## Gaps the prototype must fill (not in the reference)

Primary-action states (install, update, downloading, paused, verifying, repair, ready, running, error, disabled), progress, settings screen, title bar and window controls, any motion, hover/focus/pressed states, empty/offline news, first-run, and every accessibility consideration (the reference's grey-on-black nav and tiny tag text have not been checked for contrast).
