from PIL import Image

# Source: the chosen bottom-right cell of the contact sheet
grid = Image.open('icons-bar-xx-right.png').convert('RGB')
cell = grid.crop((1410, 1420, 1940, 1950)).convert('RGB')
w, h = cell.size

GREEN = (64, 175, 107)
WHITE = (255, 255, 255)

# Build a clean full-bleed green square, painting the white mark where the
# source pixels are near-white. Anti-alias via alpha based on luminance.
px = cell.load()
out = Image.new('RGB', (w, h), GREEN)
opx = out.load()
for y in range(h):
    for x in range(w):
        r, g, b = px[x, y]
        # whiteness: high when pixel is bright and low-saturation (the mark)
        mn = min(r, g, b)
        if mn > 150:
            t = min(1.0, (mn - 150) / 80.0)  # 0..1 blend toward white
            opx[x, y] = (
                int(GREEN[0] + (WHITE[0] - GREEN[0]) * t),
                int(GREEN[1] + (WHITE[1] - GREEN[1]) * t),
                int(GREEN[2] + (WHITE[2] - GREEN[2]) * t),
            )

icon = out.resize((1024, 1024), Image.LANCZOS)
icon.save('icon-final.png')
print('saved icon-final.png', icon.size)
