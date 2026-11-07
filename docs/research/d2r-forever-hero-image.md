# Hero image for Diablo II: Resurrected Forever

Research for [#22](https://github.com/synpoox/resurrected-launcher/issues/22). Checked 2026-11-07.

## Question

Which official Blizzard D2R promotional image should replace the Necromancer class strip as the Forever hero? It must work at full bleed in variant D, leave room for the two-line title on the left, and must not be a gameplay screenshot.

How variant D uses the art: the hero is the `1fr` column next to a 220px sidebar (`.D { grid-template-columns: 220px 1fr }`, `prototypes/launcher-shell-prototype/src/styles.css:133`). That makes it about **1060×720** in a 1280×720 window and **804×640** in a 1024×640 window. The `.art` layer uses `background-size: cover` (`styles.css:34`), and a product can override this with `artSize`/`artPos` (`src/data.js`, `variants.jsx:6-7`). `.D .shade` darkens the bottom and the left half (`styles.css:78`), and the title and Play button sit bottom-left. The subject should therefore sit right of centre and in the upper two-thirds. The art is hotlinked rather than copied (`data.js:5`).

Crop maths for `cover`:

- A 16:9 image (1920×1080) renders 1280 px wide at 1060×720, so 17% is cropped. At 804×640 it renders 1138 px wide, so 29% is cropped.
- A 21:9 image (5120×2160) renders 1707 px wide at 1060×720, so 38% is cropped. At 804×640 it renders 1517 px wide, so 47% is cropped.

## Recommendation

1. **Amazon "Mummy Battle" desktop wallpaper**. This has the best composition. The left half is dark, empty stone and the two figures sit around 50–75% across. The catch is hosting: the press center does not offer a stable public URL at hero size (see C1). The owner has to decide whether to self-host a downloaded copy.
2. **D2R 30th Anniversary key art (Diablo), Blizzard CDN**. This is the best image that can be hotlinked today, at the same CDN as the current art. It is very strong, but it fills the whole frame, so the title sits over Diablo's body and arm. It leans on the shade.
3. **"Two heroes and a wolf" (Assassin and Druid at the gate), Blizzard CDN**. It can be hotlinked and is a calmer scene. The Assassin stands left of centre, though, so she competes with the title.

If hotlinking from Blizzard's CDN is a hard rule, take #2. If the owner accepts self-hosting, take #1.

## Candidates

### C1. Amazon "Mummy Battle" desktop wallpaper (top pick)

- **Owner/source:** Blizzard press center, D2R product page, **artwork** tab, collection "Diablo II: Resurrected - Desktop Wallpapers", dated 9/14/2021: https://blizzard.gamespress.com/Diablo-II-Resurrected. The artwork tab loads through `POST /Items/FetchTabContent?productId=41` with `fileCollectionTypeId=5`, then `/Files/AjaxThumbnailFetch?...Id=12541`.
- **File:** `Amazon_MummyBattle_2D_Desktop.jpg`. The press center lists it as "JPEG image|5120 x 2160" (8.9MB). **Verified 5120×2160** by reading the JPEG header of the downloaded file.
- **Direct URL:**
  - There is no stable full-size public URL.
  - The asset path is `/blob/BlizzardLive/2021/09/14172731-dcde4064-939e-4f1e-888e-4b9fa8776b20/Amazon_MummyBattle_2D_Desktop.jpg`. Fetching it on `blizzard.gamespress.com` returns 403, which suggests a press login is needed.
  - The full-size preview is an Azure SAS URL on `fservus20221227.blob.core.windows.net` that expires about 3 hours after the listing is fetched (`se=` parameter).
  - The `imguscdn.gamespress.com` thumbnails are signed (`sky=`). Only `w=250` is signed for this file, and changing `w` returns 403 (tested).
- **Credit:** "Diablo II: Resurrected desktop wallpaper © Blizzard Entertainment" (the press-center item is sponsored by "Blizzard").
- **Composition:**
  - A dark sandstone tomb interior. Heavy carved pillars and skulls fill the left 50%, in shadow.
  - A cold blue shaft of light falls through a doorway at about 50–65%, where a red-wrapped mummy lunges.
  - The Amazon, with shield and spear in warm gold and bronze, stands at about 62–75% across, from top third to bottom.
  - The mood is moody teal shadows with warm figure highlights. It is not gameplay; it is painted promo art.
- **Suggested CSS:** `artPos: '60% center'`, `artSize: 'cover'`. The Amazon lands at about 68% across at both window sizes. The left 45% is the pillar wall behind the title.

### C2. D2R 30th Anniversary key art, no logo (best hotlinkable)

- **Owner/source:**
  - Blizzard News, the textless card art for "Celebrate Diablo's 30th Anniversary with Legacy of Terror" on https://news.blizzard.com/en-us/diablo2. The listing JSON field is `staticTextlessAsset`.
  - Article: https://news.blizzard.com/en-us/article/24315743/celebrate-diablos-30th-anniversary-with-legacy-of-terror
- **Direct URL:** https://blz-contentstack-images.akamaized.net/v3/assets/blt286175c11a6b3f4c/bltbc24705a63d11582/6aee1babc978be3136a3cfe5/DIA_D2R_30th_Anniversary_Key_Art_1920x1080_nologo_GG01.png
- **Size:** **verified 1920×1080**. The download returns `Content-Type: image/jpeg`, 903,391 bytes, even though the name ends in `.png`.
- **Credit:** "Diablo II: Resurrected 30th Anniversary key art © Blizzard Entertainment".
- **Composition:**
  - Diablo, red and armoured with bone spines, fills the frame. His head is at about 48% across and a third down, and his claws reach both edges.
  - Behind him is a glowing red-orange arcane circle. The mood is hot red and black and very aggressive.
  - There is no true negative space: the left third holds his raised arm.
- **Suggested CSS:** `artPos: '20% 30%'`, `artSize: 'cover'`. This pushes the head to about 55–60% across. Expect the title to overlay his arm; the `.D .shade` left gradient carries it.

### C3. "Two heroes and a wolf preparing for battle in a snowstorm outside a dark gate"

- **Owner/source:**
  - Blizzard News, the textless card art for "Diablo II: Resurrected Ladder Season 9 Has Concluded" on https://news.blizzard.com/en-us/diablo2. The title quoted above is the image's own alt text.
  - Article: https://news.blizzard.com/en-us/article/24158875/diablo-ii-resurrected-ladder-season-9-has-concluded
  - The same painting appears as a 5120×2160 press wallpaper, `Assassin_Druid_TheGate_2D_Desktop.jpg`, in the press-center collection from C1 (verified 5120×2160). That version has the same hosting limits as C1.
- **Direct URL:** https://blz-contentstack-images.akamaized.net/v3/assets/blt286175c11a6b3f4c/bltadd076c5d5667be3/673772c1d7f032abfcfd04a0/DIA---D2R-LS9-2.9.0-Bnet-textless-1920x1080-TM01_V2.png
- **Size:** **verified 1920×1080** (served as JPEG, 532,594 bytes).
- **Credit:** "Diablo II: Resurrected key art © Blizzard Entertainment".
- **Composition:**
  - The Assassin, dark leather with claw blades, stands at about 25–50% across. The Druid (red hair, furs) is at about 47–68%, and a snarling wolf at about 60–90%.
  - A brazier and a portcullis gate are on the far left. The right 25% is an empty, snowy, dark sky over spiked palisades.
  - The palette is cold grey-blue with warm skin and fire accents. The figures run full height, so the Assassin's legs sit in the title zone.
- **Suggested CSS:** `artPos: '100% center'`, `artSize: 'cover'`. This crops the brazier and puts the Assassin at about 10–40% of the hero. It is weaker for the title than C1.

### Also viewed (lower fit)

| Image | Source and URL | Size (verified) | Why lower |
| --- | --- | --- | --- |
| Sorceress "Sewer Magic" wallpaper | Press center, same collection as C1 (`Sorceress_SewerMagic_2D_Desktop.jpg`) | 5120×2160 | The composition is as good as C1: a dark sewer with the Sorceress about 65% across. But the Sorceress is already the 09 hero (`data.js:46`), and hosting has the same limits as C1. |
| Diablo "Lord of Terror" wallpaper | Press center, same collection (`Diablo_LordOfTerror_2D_Desktop.jpg`) | 5120×2160 | Diablo is centred (head about 55%) in a blue cathedral and fills the frame. The Blizzard CDN copy (Patch 2.9.0 card, `.../blt15f1e90ef79b12e4/.../DIA_D2R_DIA_D2R_Patch2.9.0_bnet_1920x1080_TXTLESS_LL01.png`, 1920×1080) has a baked-in stone frame. |
| D2R Mephisto art | Press center, "Mephisto Art" collection (`D2R_-_Mephisto.png`) | 3286×1408 | Mephisto is centred with five heroes, and the scene is busy edge to edge. |
| Ladder S8 "ArtOnly" card (Mephisto) | news.blizzard.com textless card, e.g. `.../blt031721e2c8718219/69fe19abfb255521a82fed51/DIA_D2R_Ladder_S8_BNET_ArtOnly_1920x1080.png` | 1920×1080 | The stone frame is baked in, and all 7 CDN copies are the same framed image. |
| Reign of the Warlock launch-trailer still | news.blizzard.com card, `.../bltf59caf9a857cebc0/698c812fa9cfc46516232c82/DIA-2025-19306_D2R_LaunchTrlr_JD_HRZ_Still_01.png` | 1920×1080 | A cinematic frame with the subject on the left and a burning village on the right. It is Warlock-themed, which the PD2R hero already uses. |
| Steam library hero | Steam app 2536520, published by "Blizzard Entertainment, Inc." (Steam `appdetails`); asset hash from `IStoreBrowseService/GetItems`: https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/2536520/a06b367f20fae83aa5d7d533dae5ee9385dde866/library_hero_2x.jpg | 3840×1240 | The composition is good, with a dark left and figures at about 35–60%. But it shows the Warlock plus a Diablo skull, which repeats PD2R's art. |
| Champions United, Barbarian Battle Ready wallpapers | Press center, same collection | 5120×2160 each | Champions United is a small centred group on black. The Barbarian is on the left, where the title goes. |

## Sources checked that yielded nothing usable

- `https://diablo2.blizzard.com/en-us/`: only class strips (for example `amazon_background_2600.webp`), Warlock key art and gameplay images. `/en-us/media` and `/en-us/fankit` return 404.
- Battle.net shop (`shop.battle.net` / `us.shop.battle.net` D2R product): redirects to `?error=Configuration` when fetched without a browser.
- `https://www.blizzard.com/en-us/games/d2r`: 404.
- Xbox store (`xbox.com/.../diablo-ii-resurrected/...`): 404 for the product id tried. Not pursued further.
- Steam `page_bg_raw.jpg` (1438×810): a smoke texture only.
- PlayStation and Nintendo stores: not checked.

## Open questions

- **Usage terms.** The press center shows no press-kit usage terms. Its footer links only Blizzard's general [Terms of Use for Blizzard's Websites](https://www.blizzard.com/legal/511dbf9e-2b2d-4047-8243-4c5c65e0ebf1/) and the [legal hub](https://www.blizzard.com/legal/). No fan-content or press licence was found for a third-party mod launcher. The owner should treat all of these images as © Blizzard with no granted licence, whether hotlinked or self-hosted.
- **Hosting C1.** Using the press wallpaper means downloading it with a press-center session and hosting a copy somewhere outside this repo. Hotlinking an expiring SAS URL will break within hours. This is an owner decision.
- Should Forever avoid Diablo himself? He also appears on the PD2R and Steam Warlock art. Choosing C1 or C3 keeps the three mods visually distinct.
