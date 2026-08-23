#!/usr/bin/env python3
"""Boot FreeDOS + G.EXE under qemu, leave it running with the gdbstub open."""
import os, socket, subprocess, sys, time

SOCK = "/tmp/finkel/gq.sock"
KEYMAP = {':': 'shift-semicolon', '\n': 'ret', ' ': 'spc', '.': 'dot'}

class Mon:
    def __init__(self, path):
        for _ in range(200):
            try:
                self.s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                self.s.connect(path); break
            except OSError:
                time.sleep(0.1)
        else: raise RuntimeError("monitor never came up")
        self.s.settimeout(4); time.sleep(0.5); self.drain()
    def drain(self):
        out = b""
        try:
            while True:
                b = self.s.recv(65536)
                if not b: break
                out += b
                if out.rstrip().endswith(b"(qemu)"): break
        except socket.timeout: pass
        return out.decode("utf-8", "replace")
    def cmd(self, c):
        self.s.sendall((c+"\n").encode()); time.sleep(0.3); return self.drain()
    def type(self, s):
        for ch in s:
            self.cmd("sendkey " + KEYMAP.get(ch, ch)); time.sleep(0.15)
    def screen(self):
        raw = self.cmd("xp /4000xb 0xb8000"); data = bytearray()
        for line in raw.splitlines():
            if ":" not in line: continue
            for tok in line.split(":",1)[1].split():
                if tok.startswith("0x"): data.append(int(tok,16) & 0xff)
        txt = bytes(data[0::2]).decode("cp866","replace")
        return "\n".join(txt[r*80:(r+1)*80].rstrip() for r in range(25))
    def wait_for(self, needle, timeout=60):
        end = time.time() + timeout
        while time.time() < end:
            s = self.screen()
            if needle in s: return s
            time.sleep(1.5)
        raise TimeoutError(f"never saw {needle!r}; last screen:\n{self.screen()}")

def start():
    if os.path.exists(SOCK): os.unlink(SOCK)
    q = subprocess.Popen([
        "qemu-system-i386",
        "-drive","file=boot.img,format=raw,if=floppy","-boot","a",
        "-hda","fat:rw:gamedir","-display","none","-m","16",
        "-monitor",f"unix:{SOCK},server=on,wait=off","-s",
    ], stdout=open("q.log","w"), stderr=subprocess.STDOUT)
    open("qemu.pid","w").write(str(q.pid))
    m = Mon(SOCK)
    m.wait_for("Do you want to proceed")
    m.type("n\n")
    m.wait_for(">", timeout=60)          # a DOS prompt of some shape
    m.type("c:\n"); time.sleep(1)
    m.type("g\n")
    m.wait_for("Версия 1.02", timeout=60)
    return q, m

if __name__ == "__main__":
    q, m = start()
    print(m.screen())
    print("[game up; gdbstub :1234; pid", q.pid, "]")
