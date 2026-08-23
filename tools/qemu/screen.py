import sys, re
# turn qemu 'xp /Nxb 0xb8000' output into 80x25 text
data = bytearray()
for line in sys.stdin:
    m = re.match(r'^[0-9a-fx]+:\s+(.*)$', line.strip())
    if not m: continue
    for tok in m.group(1).split():
        if tok.startswith('0x'):
            data.append(int(tok, 16) & 0xff)
cp866 = bytes(data[0::2])          # even bytes are characters
txt = cp866.decode('cp866', errors='replace')
for r in range(25):
    row = txt[r*80:(r+1)*80].rstrip()
    if row: print(row)
