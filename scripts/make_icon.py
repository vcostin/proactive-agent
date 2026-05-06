import struct, zlib, os

def crc(d):
    c = 0xFFFFFFFF
    for b in d:
        c ^= b
        for _ in range(8):
            c = (c >> 1) ^ (0xEDB88320 if c & 1 else 0)
    return (~c) & 0xFFFFFFFF

def chunk(t, d):
    b = t.encode() + d
    return struct.pack('>I', len(d)) + b + struct.pack('>I', crc(b))

w, h = 512, 512
ihdr = struct.pack('>IIBBBBB', w, h, 8, 2, 0, 0, 0)
rows = bytearray()
for y in range(h):
    rows.append(0)
    for x in range(w):
        cx, cy = x - w//2, y - h//2
        if (cx*cx + cy*cy) < (w*0.4)**2:
            rows += bytes([0x4a, 0x9e, 0xff])
        else:
            rows += bytes([0x1a, 0x1a, 0x2e])
data = zlib.compress(bytes(rows))
png = b'\x89PNG\r\n\x1a\n' + chunk('IHDR', ihdr) + chunk('IDAT', data) + chunk('IEND', b'')
out = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), 'app-icon.png')
open(out, 'wb').write(png)
print('written', len(png), 'bytes to', out)
