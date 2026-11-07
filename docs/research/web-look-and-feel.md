# resurrected-web look-and-feel inventory

Research for [#14](https://github.com/synpoox/resurrected-launcher/issues/14), part of map [#12](https://github.com/synpoox/resurrected-launcher/issues/12). Checked 2026-11-07 against [synpoox/resurrected-web](https://github.com/synpoox/resurrected-web) (the repository earlier ADRs call `project-web-clean`; GitHub redirects that name).

## Sources and commits

The web repository's history was squashed into snapshot commits on 2026-11-06. Commit SHAs named in its prose (for example `46909c7`) do not exist in the repository.

| Ref | Commit | What it contains |
| --- | --- | --- |
| `main` | [`1a60737`](https://github.com/synpoox/resurrected-web/commit/1a60737851ecc3371bde78ec25b824490667c69a) | Single page, ADRs 0001–0002, `themes.css`, `styles.css`, `mark.svg`, Warlock hero |
| `codex/launch-page-design-comparison` | [`7c487de`](https://github.com/synpoox/resurrected-web/commit/7c487de0e3345229f1d51b456b144a2128ba745d) | Empty commit on top of `1a60737`; its tree is identical to `main` (`git diff 1a60737 7c487de` is empty) |
| `codex/add-discord-community` (PR #3) | [`d2615b6`](https://github.com/synpoox/resurrected-web/commit/d2615b6c13e234e255c1d074dbb3f6a800d7888a) | Discord link in nav and hero; one line of CSS |
| `codex/guides-tools-naming` (PR #4) | [`155ecee`](https://github.com/synpoox/resurrected-web/commit/155eceed55ba424477f4316a8ffddb0844d5445d) | Built on `d2615b6`; adds `branding.json`, `hero-sources.json`, `carousel.js`, Forever/09 heroes, ADRs 0003–0004, guide/tools pages and CSS |

Below, `@M` means `1a60737`, and `@S` means `155ecee`. The branch PR #4 leaves `site/themes.css` and `site/assets/mark.svg` unchanged: `git diff 1a60737 155ecee` has no lines for those files. Token values are therefore the same at both commits.

PR #3 and PR #4 are open and unmerged. Neither has any comments, review comments or reviews (`gh api .../pulls/{3,4}/comments` and `.../issues/4/comments` each return 0). Owner decisions are recorded only in the ADRs, the PR bodies, [web issue #1](https://github.com/synpoox/resurrected-web/issues/1) (with its owner comment) and [web issue #2](https://github.com/synpoox/resurrected-web/issues/2).

## `launcher` preset (the default)

Source: [`site/themes.css@S`](https://github.com/synpoox/resurrected-web/blob/155eceed55ba424477f4316a8ffddb0844d5445d/site/themes.css), line 2 (identical at `@M`). The preset applies to `:root` and to `html[data-theme="launcher"]`, with `color-scheme: dark`.

| Token | Value | Use (from `styles.css`) |
| --- | --- | --- |
| `--bg` | `#101011` | page, `.shell`, hero gradient base |
| `--panel` | `#1d1d1e` | `.about`, cards, secondary-button hover |
| `--text` | `#f4f1ed` | body text |
| `--muted` | `#b1adac` | nav links, secondary copy |
| `--accent` | `#e3353d` | rail top border (2px), active nav underline, `.brand-mark`, card top border |
| `--line` | `#353233` | 1px borders and dividers |
| `--button` | `#c72d37` | primary button, skip link |
| `--button-hover` | `#e13742` | primary button hover |
| `--detail` | `#eb626b` | numbers, status dots, arrows, small highlights |

Font: `font-family: Arial, Helvetica, sans-serif`. The preset loads no web font. The ADR 0002 review page describes the launcher look as "Bold sans-serif type, charcoal panels, brighter crimson" ([`review/index.html@S`](https://github.com/synpoox/resurrected-web/blob/155eceed55ba424477f4316a8ffddb0844d5445d/review/index.html)).

## `d2r` preset (kept as the alternative)

Source: the same file, lines 3 and 5. Its tokens are `--bg #0d0c0b`, `--panel #191714`, `--text #ede5d5`, `--muted #b5ac9e`, `--accent #846542`, `--line #383027`, `--button #8f2927`, `--button-hover #a7352f` and `--detail #baa174`.

It also overrides these selectors:
- rail `#090908f5`
- ribbon `#7e2625`
- `h1` uses `Georgia, 'Times New Roman', serif`, weight 400, `letter-spacing: -2px` and `text-shadow: 0 3px 24px #0008`
- hero image `filter: saturate(.72) brightness(.8)`
- hero gradient rebased on `#0d0c0b`
- primary button `border-color: #bd6350` and `box-shadow: inset 0 0 18px #5d17174d`
- install card `linear-gradient(135deg,#2c211c,#191512)`

Map #12 keeps this preset out of scope for the launcher.

## Shared layout values (`site/styles.css`)

Source: [`site/styles.css@M`](https://github.com/synpoox/resurrected-web/blob/1a60737851ecc3371bde78ec25b824490667c69a/site/styles.css), with additions at [`@S`](https://github.com/synpoox/resurrected-web/blob/155eceed55ba424477f4316a8ffddb0844d5445d/site/styles.css).

- **Colours outside the tokens.** `styles.css` uses several literals that the tokens do not cover:
  - rail `#0b0b0cf5`; hero fallback `#0a0a0c`
  - ribbon `#b52b35`; selection `#94242c`
  - hero copy `#cbc3c2`, `#d3c8c6`, `#b3abaa`, `#aaa09b` and `#d8d1cf`
  - hero-bottom rule `#ffffff24`; text-link underline `#69605f`
  - install card `linear-gradient(135deg,#352125,#201b1d)`
  - focus ring `#fff`; secondary button `#10101180` (`@S` line 2)
  - hero overlay `linear-gradient(90deg,#101011 0%,#101011f5 28%,#101011a3 46%,#10101100 68%), linear-gradient(0deg,#101011 0%,#10101100 24%,#10101100 80%,#10101155)`
- **Type scale.**
  - body is 15px with line-height 1.6
  - `h1` is `clamp(40px,4.4vw,67px)`, weight 900, `letter-spacing: -2.5px`, line-height 1.06, `text-wrap: balance`
  - `h2` is `clamp(26px,2.6vw,39px)`, weight 800, `-1.3px`; `h3` is 21px with `-.5px`
  - hero description is 14px with line-height 1.75; buttons are 12px, weight 700, `.4px`
  - nav is 12px; the identity is 15px, weight 900, `1px` tracking
  - uppercase micro-labels are 8–9px, weight 700–800, with 1.5–2px tracking (`.hero-top`, `.ribbon`, `.section-label`); they shrink to 7px at ≤650px
  - numbers and step labels use `monospace` at 10–12px
- **Spacing.**
  - horizontal page gutter is `5.3%`, or 24px at ≤650px
  - rail is `min-height: 92px` with padding `20px 5.3%`
  - hero is `min-height: 710px` with padding `28px 5.3% 25px`; `.hero-copy` has `max-width: 610px` and padding `78px 0 85px`
  - buttons have padding `15px 22px`, `min-height: 52px` and gap 22px
  - sections have padding `48px 5.3% 54px`; cards have padding `28px 30px`; grid gaps are 24px or 70px
  - `@S` adds a bounded reading column of `min(100%,900px)` with 36px padding, and controls at `min-height: 44px` (`.guide-next`, tabs)
- **Radii.** The launcher look is square-cornered. The only radius in either file is `.tool-card{border-radius:3px}` at `@S`. The ribbon is a slanted parallelogram, `clip-path: polygon(4% 0,100% 0,96% 100%,0 100%)`.
- **Shadows.** The `launcher` preset and `styles.css` use none. Depth comes from 1px `--line` borders, the 2px `--accent` top border on the rail and the install card, and gradients. Only the `d2r` preset adds `text-shadow` and an inset `box-shadow`.
- **Motion and focus.**
  - buttons use `transition: background .15s, transform .15s`; the primary hover is `translateY(-2px)`
  - links show `outline: 3px solid #fff; outline-offset: 6px` on focus
  - `scroll-behavior: smooth`
  - `@media (prefers-reduced-motion: reduce)` removes the smooth scroll, the transition and the hover lift
- **Width.** `.shell` has `max-width: 1720px`. Breakpoints are at 1050px (`@S`), 900px and 650px.

## Identity assets

- **`branding.json`** ([`@S`](https://github.com/synpoox/resurrected-web/blob/155eceed55ba424477f4316a8ffddb0844d5445d/branding.json); absent at `@M`). Its entries are:
  - `status: "provisional-preview"`, `umbrella: "Project Resurrected"`
  - `pd2r: "Project Diablo 2: Resurrected"`, `project_forever: "Diablo II: Resurrected Forever"`, `project_09: "Diablo II: Resurrected 09"`, `tools: "D2R Tools"`

  Web ADR 0004 says that this file owns the labels, that the order is PD2R, Forever, 09, Tools, and that "branding is not finalized" ([ADR 0004@S](https://github.com/synpoox/resurrected-web/blob/155eceed55ba424477f4316a8ffddb0844d5445d/docs/adr/0004-provisional-umbrella-and-project-directory.md), section "Later owner direction").
- **`mark.svg`** ([`@S`](https://github.com/synpoox/resurrected-web/blob/155eceed55ba424477f4316a8ffddb0844d5445d/site/assets/mark.svg), identical at `@M`). It is a 48×48 viewBox with a solid `#121213` square. On top is an outlined hexagon (`M24 3 43 14v20L24 45 5 34V14Z`) containing an "A" (`m15 33 9-19 9 19M19 26h10`), drawn in `stroke="#f4f1ed"` at `stroke-width="2"` with no fill. The colours are hard-coded, not tokens. The site uses it only as the favicon (`<link rel="icon">` in [`content/index.html@S`](https://github.com/synpoox/resurrected-web/blob/155eceed55ba424477f4316a8ffddb0844d5445d/content/index.html)). The visible rail mark is a text `/` glyph (`.brand-mark`, 43px, weight 400, `--accent`) next to the umbrella name.
- **`hero-sources.json`** ([`@S`](https://github.com/synpoox/resurrected-web/blob/155eceed55ba424477f4316a8ffddb0844d5445d/site/assets/hero-sources.json)). It lists only two entries:
  - `d2r-forever-hero.webp`: the Blizzard CDN `necromancer_background_2600.webp`
  - `d2r-09-hero.webp`: the CDN `paladin_background_2600.webp`

  The PD2R Warlock image (`d2r-warlock-hero.png`, 1920×1080) is credited only in the README, which gives the Battle.net product page and a Blizzard CDN `KEYART2` PNG ([`README.md@S`](https://github.com/synpoox/resurrected-web/blob/155eceed55ba424477f4316a8ffddb0844d5445d/README.md), line 33). The on-page credit reads "Official D2R promotional artwork © Blizzard Entertainment · Not mod gameplay" (`content/index.html@S`, line 103).

## Hero carousel behaviour

Source: [`site/carousel.js@S`](https://github.com/synpoox/resurrected-web/blob/155eceed55ba424477f4316a8ffddb0844d5445d/site/carousel.js), with markup in `content/index.html@S` lines 33–49.

- **Slides.** There are three, in this order:

  | Product | Art | `object-position` | Headline | Status | Action |
  | --- | --- | --- | --- | --- | --- |
  | Project Diablo 2: Resurrected | Warlock | `55% center` | "The game you know. / A world reborn." | `PLAYABLE ALPHA` | Install |
  | Diablo II: Resurrected Forever | Necromancer | `center` | "A different future. / A resurrected world." | `PROJECT OVERVIEW` | "Explore Forever" |
  | Diablo II: Resurrected 09 | Paladin | `center` | "An earlier era. / A world revisited." | `PROJECT OVERVIEW` | "Explore 09" |

  Each slide also sets a ribbon label, a description, the `FEATURED / <NAME>` top line and alt text.
- **Transition.**
  1. The script preloads every image and waits for `decode()` on the next one.
  2. The art fades out over 300ms while the copy fades out and moves `translateY(0→8px)`.
  3. The content swaps.
  4. The art and copy fade back in over 450ms (1.5×). A `moving` flag blocks overlapping transitions.
  5. With `prefers-reduced-motion: reduce`, the duration is 0.
- **Auto-advance.** The hero advances every 8000ms, unless the user has hovered it, focus is inside it, reduced motion is set or `document.hidden` is true. The script declares `manualPause` but never sets it. ADR 0004 records the design as "without a visible pause button".
- **Controls.** The controls are `←` / `N / 3` / `→` buttons (`.carousel-controls`). They are `hidden` without JavaScript, so the no-JS page is a static PD2R hero. The buttons use the panel background, a 1px line border, padding `9px 14px` and a white 3px focus ring. The section has `aria-roledescription="carousel"` and `aria-label="Featured mods"`.
- **Layout.** The art sits at `inset: 0 0 0 30%`, so it fills the right 70%. A left-to-right gradient keeps the copy column readable. The copy has one headline and one primary action, plus a secondary Discord button at `@S`.

## Owner decisions: approved and rejected

| Decision | Status | Source |
| --- | --- | --- |
| Use the Riot launcher (Cutillas) as direct compositional inspiration: cinematic hero, one hero sentence, concise panels, clear primary action; no social/news/account features | Accepted | [ADR 0002@M](https://github.com/synpoox/resurrected-web/blob/1a60737851ecc3371bde78ec25b824490667c69a/docs/adr/0002-shared-design-and-switchable-themes.md) |
| Prefer `launcher` as the default; keep `d2r` as an easy alternative; "not a permanent brand commitment" | Accepted, reversible | ADR 0002@M; [web issue #1 owner comment](https://github.com/synpoox/resurrected-web/issues/1#issuecomment-6698344531) |
| One page source, shared `styles.css`, scoped presets in `themes.css`; no shared runtime code with the launcher; "Coordinate later shared tokens/assets explicitly" | Accepted | ADR 0002@M, "Cross-project boundary" |
| Navigation at the top ("moved to the top at the owner's request") | Accepted, web only | README.md@S line 31; ADR 0002@M |
| Official Warlock art, credited, never presented as mod gameplay | Accepted | ADR 0002@M "Content and artwork" |
| Remove the large product-name heading in favour of the hero sentence | Accepted | ADR 0002@M |
| Three-mod hero rotation PD2R → Forever → 09 with distinct Warlock/Necromancer/Paladin art; previous/next, hover/focus pause, reduced motion; no visible pause button | Accepted direction | ADR 0004@S "Later owner direction"; [PR #4 body](https://github.com/synpoox/resurrected-web/pull/4) |
| A visible pause button | Dropped: content-verification at `@S` line 66 mentions "previous/next/pause buttons"; ADR 0004 and the code have none | [content-verification.md@S](https://github.com/synpoox/resurrected-web/blob/155eceed55ba424477f4316a8ffddb0844d5445d/docs/research/content-verification.md); `carousel.js@S` |
| Bounded single reading column (`min(100%,900px)`) for Getting Started; sequential sections instead of side-by-side panels | Accepted | ADR 0004@S "Decision" |
| Tools page: compact 3/2/1-column cards (replaces the earlier single column) | Accepted | ADR 0004@S |
| Tallest-panel / equal-height installation experiment | **Rejected**, reverted ("`46909c7`"; that commit was lost in the squash) | ADR 0004@S; `docs/agents/project-context.md@S` line 35 |
| Compact step navigator with Next buttons | Direction selected; the rendering "awaits visual/keyboard acceptance" | ADR 0004@S; PR #4 body |
| Umbrella label "Project Resurrected" (alternative: "The Resurrected Project") | Provisional, explicitly not final | ADR 0004@S "Owner feedback" |
| Discord as a secondary hero action, Install as the primary | Accepted in PR #3 (unmerged) | [PR #3 body](https://github.com/synpoox/resurrected-web/pull/3) |

None of these has browser visual, keyboard or contrast acceptance. Web issue #2, the web issue #1 owner comment and the PR #3 and PR #4 bodies each say that browser QA is unverified.

The `codex/launch-page-design-comparison` branch adds no experiments beyond `main`. The comparison it refers to is generated by `scripts/build-previews.py` from one source, plus the static `review/index.html`. That page labels launcher-inspired "SELECTED DIRECTION" (`review/index.html@M`).

## Recommended pin

Pin **`155eceed55ba424477f4316a8ffddb0844d5445d`**, the head of `codex/guides-tools-naming` and web PR #4. It is the only commit that contains every input map #12 names:
- the `launcher` tokens
- `branding.json`
- `hero-sources.json` with the Forever and 09 heroes
- `mark.svg`
- `carousel.js`
- ADRs 0002–0004

Its `themes.css` and `mark.svg` are byte-identical to `main@1a60737`, so the tokens would be the same if the launcher pinned `main`. `main` lacks the branding and hero inputs, though. The risks are:
- PR #4 is unmerged and could be force-pushed. The snapshot squash shows that history is rewritten.
- If the launcher copies from it, record the SHA and the copied values in the launcher spec. Do not link to the branch.

## Open decisions for the shell prototype

- **Uncovered colours.** Decide which `styles.css` literals become launcher tokens (ribbon `#b52b35`, rail `#0b0b0cf5`, hero copy greys, focus `#fff`, `#ffffff24` rule) and which are dropped. The web has nine tokens only.
- **Font.** The web uses system Arial with no web font. Keep it, or bundle a font in the app.
- **Radii and shadows.** The web is square with no shadows (3px on tool cards only). Confirm this for launcher controls, cards and the title bar.
- **Navigation.** The web moved navigation to the top, but the launcher map wants a product sidebar. ADR 0002 says web choices do not prescribe launcher navigation.
- **Brand mark.** Choose between the rail `/` glyph and `mark.svg` for the title bar and window icon. `mark.svg` has a hard-coded `#121213` background and `#f4f1ed` stroke, and does not use `--accent`.
- **Hero.** In the launcher a hero is selected per product from the sidebar. Decide whether any rotation survives. If it does, decide whether it reuses the web's 300/450ms fade with an 8px lift and an 8s interval.
- **Warlock credit.** The Warlock hero's source is missing from `hero-sources.json` and appears only in the README. The launcher needs its own credit record for all three images.
- **Micro-labels.** 7–9px uppercase labels and `#aaa09b`-on-dark text have no contrast or size check. Set a minimum size in the launcher at 1280×720 and 1024×640.
- **Fixed sizes.** Web sizes depend on the viewport (`clamp`, `5.3%` gutters, `min-height: 710px` hero). The launcher needs fixed px values for a 1280×720 window.
