"""Measure every text box with real Calibri metrics and render a layout preview.

For environments with no slide renderer: run `QA=1 node build.js`, then this.
Flags any text box whose wrapped text is taller than the box, or that leaves
the slide. Needs Pillow and PowerPoint's bundled Calibri."""
import json, pathlib
from PIL import Image, ImageDraw, ImageFont

HERE = pathlib.Path(__file__).parent
FONTS = pathlib.Path("/Applications/Microsoft PowerPoint.app/Contents/Resources/DFonts")
DPI = 96
W, H = 13.333, 7.5
_cache = {}

def font(pt, bold):
    key = (round(pt * DPI / 72), bold)
    if key not in _cache:
        _cache[key] = ImageFont.truetype(str(FONTS / ("Calibrib.ttf" if bold else "Calibri.ttf")), key[0])
    return _cache[key]

def wrap(text, f, width_px):
    lines = []
    for para in text.split("\n"):
        words, cur = para.split(" "), ""
        if para == "":
            lines.append(("", True)); continue
        for w in words:
            trial = (cur + " " + w).strip()
            if f.getlength(trial) <= width_px or not cur:
                cur = trial
            else:
                lines.append((cur, False)); cur = w
        lines.append((cur, True))
    return lines

def px(v): return int(round(v * DPI))

def hexcol(c, default="000000"): return "#" + (c or default)

problems = []
slides = json.load(open(HERE / "qa.json"))
out = HERE / "preview"; out.mkdir(exist_ok=True)
for si, sl in enumerate(slides, 1):
    bg = (sl["background"] or {}).get("color", "FFFFFF")
    img = Image.new("RGB", (px(W), px(H)), hexcol(bg))
    d = ImageDraw.Draw(img)
    for it in sl["items"]:
        o = (it.get("opts") or (it.get("a0") if it["kind"] == "addImage" else None)) or {}
        x, y, w = o.get("x", 0), o.get("y", 0), o.get("w", 0)
        h = o.get("h", 0)
        if it["kind"] == "addShape":
            fill = (o.get("fill") or {}).get("color")
            if it["shape"] == "line":
                d.line([px(x), px(y), px(x + w), px(y + h)], fill=hexcol((o.get("line") or {}).get("color")), width=2)
            else:
                d.rounded_rectangle([px(x), px(y), px(x + w), px(y + h)], radius=px(o.get("rectRadius", 0) or 0), fill=hexcol(fill))
        elif it["kind"] == "addImage":
            try:
                im = Image.open(o["path"]).convert("RGB").resize((px(w), px(h)))
                img.paste(im, (px(x), px(y)))
            except Exception as e:
                problems.append(f"slide {si}: image {o.get('path')} {e}")
        elif it["kind"] == "addChart":
            d.rectangle([px(x), px(y), px(x + w), px(y + h)], outline="#8a93a6", width=2)
            d.text((px(x) + 10, px(y) + 10), "[native chart] " + (o.get("title") or ""), fill="#5a6272", font=font(12, False))
        elif it["kind"] == "addTable":
            rows = it["a0"]; colw = o.get("colW"); rh = o.get("rowH", 0.4)
            for ri, row in enumerate(rows):
                cx = x
                for ci, cell in enumerate(row):
                    f = font(cell["options"].get("fontSize", 12), cell["options"].get("bold", False))
                    t = cell["text"]
                    if f.getlength(t) > px(colw[ci]) - 8:
                        problems.append(f"slide {si}: table cell '{t}' too wide")
                    d.text((px(cx) + 4, px(y + ri * rh) + 6), t, fill=hexcol(cell["options"].get("color")), font=f)
                    cx += colw[ci]
                d.line([px(x), px(y + (ri + 1) * rh), px(x + sum(colw)), px(y + (ri + 1) * rh)], fill="#e3e5ea")
        elif it["kind"] == "addText":
            text = it["a0"]
            pt = o.get("fontSize", 18); bold = o.get("bold", False)
            f = font(pt, bold)
            lines = wrap(text, f, px(w))
            line_h = pt * 1.2 / 72
            para_after = (o.get("paraSpaceAfter", 0) or 0) / 72
            total = sum(line_h + (para_after if end else 0) for _, end in lines)
            if total > h + 0.05:
                problems.append(f"slide {si}: text overflows its box by {total - h:.2f} in: '{text[:50]}…'")
            if x + w > W + 0.01 or y + h > H + 0.01:
                problems.append(f"slide {si}: box off the slide edge: '{text[:40]}'")
            valign = o.get("valign", "top")
            ty = y if valign == "top" else (y + (h - total) / 2 if valign == "middle" else y + h - total)
            for line, end in lines:
                lw = f.getlength(line)
                align = o.get("align", "left")
                lx = x * DPI if align == "left" else (px(x) + (px(w) - lw) / 2 if align == "center" else px(x + w) - lw)
                d.text((lx, px(ty)), line, fill=hexcol(o.get("color")), font=f)
                ty += line_h + (para_after if end else 0)
            if total > h + 0.05:
                d.rectangle([px(x), px(y), px(x + w), px(y + h)], outline="#ff0000", width=3)
    img.save(out / f"slide-{si:02d}.png")
print("\n".join(problems) if problems else "no overflow or edge problems found")
