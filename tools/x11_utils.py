"""Readiness helpers for the editor's own X11 test window."""
import ctypes as C
import ctypes.util
import time


def capture_window(window, path, width=1280, height=800):
    """Save only this test window; Xvfb uses little-endian 32-bit RGB pixels."""
    import binascii
    import struct
    import zlib
    class XImage(C.Structure):
        _fields_ = [("width", C.c_int), ("height", C.c_int), ("xoffset", C.c_int),
                    ("format", C.c_int), ("data", C.c_void_p), ("byte_order", C.c_int),
                    ("bitmap_unit", C.c_int), ("bitmap_bit_order", C.c_int),
                    ("bitmap_pad", C.c_int), ("depth", C.c_int), ("bytes_per_line", C.c_int),
                    ("bits_per_pixel", C.c_int), ("red_mask", C.c_ulong),
                    ("green_mask", C.c_ulong), ("blue_mask", C.c_ulong)]
    lib = C.CDLL(ctypes.util.find_library("X11"))
    lib.XOpenDisplay.argtypes = [C.c_char_p]
    lib.XOpenDisplay.restype = C.c_void_p
    lib.XGetImage.argtypes = [C.c_void_p,C.c_ulong,C.c_int,C.c_int,C.c_uint,C.c_uint,C.c_ulong,C.c_int]
    lib.XGetImage.restype = C.POINTER(XImage)
    lib.XDestroyImage.argtypes = [C.POINTER(XImage)]
    lib.XCloseDisplay.argtypes = [C.c_void_p]
    display = lib.XOpenDisplay(None)
    assert display
    image = None
    try:
        image = lib.XGetImage(display,int(window),0,0,width,height,C.c_ulong(-1).value,2)
        assert image
        header = image.contents
        assert (header.byte_order, header.bits_per_pixel, header.red_mask, header.green_mask, header.blue_mask) == (0,32,0xff0000,0xff00,0xff)
        raw = C.string_at(header.data,header.bytes_per_line*height)
        rows = bytearray()
        for y in range(height):
            row = raw[y*header.bytes_per_line:y*header.bytes_per_line+width*4]
            rgb = bytearray(width*3)
            rgb[0::3],rgb[1::3],rgb[2::3] = row[2::4],row[1::4],row[0::4]
            rows.append(0)
            rows.extend(rgb)
        def chunk(kind,data):
            return struct.pack(">I",len(data))+kind+data+struct.pack(">I",binascii.crc32(kind+data))
        path.write_bytes(b"\x89PNG\r\n\x1a\n"+chunk(b"IHDR",struct.pack(">IIBBBBB",width,height,8,2,0,0,0))+chunk(b"IDAT",zlib.compress(rows))+chunk(b"IEND",b""))
    finally:
        if image: lib.XDestroyImage(image)
        lib.XCloseDisplay(display)


def wait_for_editor_paint(window, process, timeout=20, height=800, background=0x141b2a):
    """Wait for the toolbar to be painted, after asynchronous resource startup."""
    lib = C.CDLL(ctypes.util.find_library("X11"))
    lib.XOpenDisplay.argtypes = [C.c_char_p]
    lib.XOpenDisplay.restype = C.c_void_p
    lib.XGetImage.argtypes = [C.c_void_p,C.c_ulong,C.c_int,C.c_int,C.c_uint,C.c_uint,C.c_ulong,C.c_int]
    lib.XGetImage.restype = C.c_void_p
    lib.XGetPixel.argtypes = [C.c_void_p,C.c_int,C.c_int]
    lib.XGetPixel.restype = C.c_ulong
    lib.XDestroyImage.argtypes = [C.c_void_p]
    lib.XCloseDisplay.argtypes = [C.c_void_p]
    display = lib.XOpenDisplay(None)
    assert display, "X11 readiness check requires a display"
    try:
        deadline = time.monotonic()+timeout
        while time.monotonic()<deadline:
            assert process.poll() is None, "Editor exited before its first frame"
            colors = []
            for x,y in [(4,4),(4,height-44)]:
                image = lib.XGetImage(display,int(window),x,y,1,1,C.c_ulong(-1).value,2)
                assert image, "Could not read the editor window"
                try: colors.append(lib.XGetPixel(image,0,0)&0xffffff)
                finally: lib.XDestroyImage(image)
            if colors == [background,background]: return
            time.sleep(0.05)
        raise AssertionError("Editor did not paint its toolbar")
    finally:
        lib.XCloseDisplay(display)


def canvas_right(path, density=1.):
    """Locate the actual canvas/inspector edge in a capture from capture_window.

    Window padding can change the inspector width from its requested default.
    Read its edge instead of making pointer coordinates depend on that default.
    """
    import struct
    import zlib
    png = path.read_bytes()
    at = 8
    compressed = []
    width = None
    while at < len(png):
        size = struct.unpack('>I', png[at:at+4])[0]
        kind, data = png[at+4:at+8], png[at+8:at+8+size]
        at += size+12
        if kind == b'IHDR':
            width = struct.unpack('>I', data[:4])[0]
        elif kind == b'IDAT':
            compressed.append(data)
    pixels = zlib.decompress(b''.join(compressed))
    stride = 1+width*3
    offset = round(50*density)*stride
    assert pixels[offset] == 0, 'Expected an unfiltered capture_window row'
    row = pixels[offset+1:offset+stride]
    background = row[:3]
    end = None
    start = None
    for x in range(width+1):
        same = x < width and row[x*3:x*3+3] == background
        if same and start is None:
            start = x
        if not same and start is not None:
            if x-start >= round(8*density):
                end = x
            start = None
    assert end is not None, 'Canvas background was not visible'
    return end/density


def request_window_quit(window):
    """Send WM_DELETE_WINDOW directly, including on Xvfb without a window manager."""
    class ClientData(C.Union):
        _fields_ = [("longs", C.c_long * 5)]

    class ClientMessage(C.Structure):
        _fields_ = [("type", C.c_int), ("serial", C.c_ulong), ("send_event", C.c_int),
                    ("display", C.c_void_p), ("window", C.c_ulong),
                    ("message_type", C.c_ulong), ("format", C.c_int), ("data", ClientData)]

    class XEvent(C.Union):
        _fields_ = [("client", ClientMessage), ("padding", C.c_long * 24)]

    lib = C.CDLL(ctypes.util.find_library("X11"))
    lib.XOpenDisplay.argtypes = [C.c_char_p]
    lib.XOpenDisplay.restype = C.c_void_p
    lib.XInternAtom.argtypes = [C.c_void_p, C.c_char_p, C.c_int]
    lib.XInternAtom.restype = C.c_ulong
    lib.XSendEvent.argtypes = [C.c_void_p, C.c_ulong, C.c_int, C.c_long, C.POINTER(XEvent)]
    lib.XFlush.argtypes = [C.c_void_p]
    lib.XCloseDisplay.argtypes = [C.c_void_p]
    display = lib.XOpenDisplay(None)
    assert display
    try:
        event = XEvent()
        event.client.type = 33
        event.client.display = display
        event.client.window = int(window)
        event.client.message_type = lib.XInternAtom(display, b"WM_PROTOCOLS", 0)
        event.client.format = 32
        event.client.data.longs[0] = lib.XInternAtom(display, b"WM_DELETE_WINDOW", 0)
        assert lib.XSendEvent(display, int(window), 0, 0, C.byref(event))
        lib.XFlush(display)
    finally:
        lib.XCloseDisplay(display)
