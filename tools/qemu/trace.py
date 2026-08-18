#!/usr/bin/env python3
"""Boot the game, break on its main ReadLn, drive input, see if the bp fires."""
import subprocess, sys, time
sys.path.insert(0, ".")
from boot import start

BASE = 0x224B0                 # linear address of Ghidra 1000:0000
READLN = BASE + 0xae63         # 1000:ae63 -- main prompt ReadLn call

q, m = start()
print("[booted]")

open("gdb.cmds", "w").write(f"""set confirm off
set pagination off
set architecture i8086
target remote :1234
break *{hex(READLN)}
continue
""")
g = subprocess.Popen(["gdb", "-batch", "-nx", "-x", "gdb.cmds"],
                     stdout=open("gdb.log", "w"), stderr=subprocess.STDOUT)
time.sleep(8)

m.type(" "); time.sleep(1.5)
m.type("1\n"); time.sleep(1.5)
for _ in range(20):
    m.type(" "); time.sleep(0.15)
m.type("2\n"); time.sleep(1.5)
m.type("Vasya\n"); time.sleep(4)

time.sleep(3)
g.terminate(); time.sleep(1)
print("=== gdb.log (tail) ===")
print(open("gdb.log").read()[-900:])
