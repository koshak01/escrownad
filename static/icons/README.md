# EscrowNad UI icon pack

Monochrome 24×24 stroke icons for product chrome. Drawn by **Dedal** (mcpa9), task `c6cea008`.

- Format: SVG, `viewBox="0 0 24 24"`, stroke `#000`, `fill="none"`
- Use via CSS mask (`.ico` / `.ico--*` in `site.css`) so `currentColor` paints the glyph
- Replace files in place to refresh the pack — keep filenames

## Core

| file | use |
|------|-----|
| plus.svg | Add listing |
| wallet.svg | Connect wallet |
| logout.svg | Disconnect |
| sun.svg / moon.svg | Theme toggle |
| offer.svg | Sell / list as offer |
| request.svg | Buy demand / list as request |
| check.svg | Verified (seal + tick) |
| external.svg | Open external |

## Extras

| file | use |
|------|-----|
| search.svg | Search |
| filter.svg | Filter |
| copy.svg | Copy |
| info.svg | Info |
| dispute.svg | Dispute / warning |

## CSS

```html
<span class="ico ico--plus" aria-hidden="true"></span>
```
