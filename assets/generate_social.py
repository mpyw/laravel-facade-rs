"""Generates assets/social-preview.svg and .png for GitHub's social preview.

Run it from the repository root, after assets/generate_logo.py:

    python3 assets/generate_social.py

It needs `rsvg-convert` for the PNG. GitHub wants 1280x640.
"""

import re
import subprocess

W, H = 1280, 640
RED = "#FF2D20"

logo = open("assets/logo.svg").read()
view_box = re.search(r'viewBox="([^"]+)"', logo).group(1)
inner = logo[logo.index(">") + 1 : logo.rindex("</svg>")]

SANS = "Helvetica Neue, Helvetica, Arial, sans-serif"
MONO = "Menlo, SF Mono, Consolas, monospace"

svg = f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {H}" width="{W}" height="{H}">
<rect width="{W}" height="{H}" fill="#0d1117"/>
<svg x="40" y="60" width="520" height="520" viewBox="{view_box}">{inner}</svg>
<g font-family="{SANS}">
  <text x="580" y="170" font-family="{MONO}" font-size="34" fill="{RED}">laravel-facade</text>
  <text x="580" y="262" font-size="72" font-weight="bold" fill="#ffffff">Facades. In Rust.</text>
  <text x="580" y="346" font-size="72" font-weight="bold" fill="#ffffff">On purpose.</text>
  <text x="580" y="420" font-size="32" fill="#8b949e">Static calls. Global state. Zero regrets.</text>
  <text x="580" y="462" font-size="32" fill="#8b949e">The borrow checker was not consulted.</text>
  <rect x="580" y="508" width="372" height="64" rx="12" fill="#161b22" stroke="#30363d" stroke-width="2"/>
  <text x="604" y="550" font-family="{MONO}" font-size="30" fill="#e6edf3">Cache::<tspan fill="{RED}">get</tspan>(<tspan fill="#a5d6ff">"key"</tspan>);</text>
</g>
</svg>
'''
open("assets/social-preview.svg", "w").write(svg)
subprocess.run(
    ["rsvg-convert", "-w", str(W), "-h", str(H), "assets/social-preview.svg", "-o", "assets/social-preview.png"],
    check=True,
)
