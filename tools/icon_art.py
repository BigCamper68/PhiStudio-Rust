"""The original IPA application artwork, shared by all platform packages."""
import struct
import zlib

def icon_rgb(size):
    # A code-drawn chart icon: opaque, without pre-rounded corners or fonts.
    pixels = bytearray()
    for y in range(size):
        for x in range(size):
            u, v = x / size, y / size
            color = (16, 23, 36)
            if .15 < u < .85 and .15 < v < .85:
                if min(abs(u - p) for p in [.25, .5, .75]) < .0025 or min(abs(v - p) for p in [.25, .5, .75]) < .0025:
                    color = (41, 58, 78)
            if .18 < u < .82 and abs(v - .72) < .006:
                color = (79, 214, 184)
            for cx, cy, width in [(.32, .55, .18), (.64, .39, .18), (.44, .24, .18)]:
                if abs(u - cx) < width / 2 and abs(v - cy) < .020:
                    color = (116, 224, 251) if abs(v - cy) < .012 else (217, 249, 255)
            pixels.extend(color)

    return bytes(pixels)

def icon_rgba(size):
    rgb=icon_rgb(size)
    return b"".join(rgb[i:i+3]+b"\xff" for i in range(0,len(rgb),3))

def icon_png(size):
    rgb=icon_rgb(size)
    rows=b"".join(b"\0"+rgb[y*size*3:(y+1)*size*3] for y in range(size))
    def chunk(kind,data):
        return struct.pack(">I",len(data))+kind+data+struct.pack(">I",zlib.crc32(kind+data)&0xffffffff)
    return (b"\x89PNG\r\n\x1a\n"+chunk(b"IHDR",struct.pack(">IIBBBBB",size,size,8,2,0,0,0))
            +chunk(b"IDAT",zlib.compress(rows,9))+chunk(b"IEND",b""))
