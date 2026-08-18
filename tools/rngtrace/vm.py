"""Headless FreeDOS guest under qemu, driven through the monitor socket.

Separate from tools/oracle/ (DOSBox-X): that path stays the sanctioned screen
capture; this one exists because gdb can attach to the guest and break on the
game's own 16-bit code.

Pitfalls already paid for (tools/qemu/README.md):
  * qemu 11 wants `-monitor unix:PATH,server=on,wait=off` (not `server,nowait`)
  * a unix socket path is capped at 108 bytes -- keep it out of long scratchpads
  * vvfat must be `-hda fat:rw:<dir>`; read-only fails
  * the VM must be killed on EVERY exit path, exceptions included
"""
import os
import socket
import subprocess
import tempfile
import time
from pathlib import Path

KEYMAP = {"\n": "ret", " ": "spc", ".": "dot", "\\": "backslash", "-": "minus",
          ":": "shift-semicolon", ",": "comma", "/": "slash"}


class MonitorError(RuntimeError):
    pass


class Monitor:
    def __init__(self, path, connect_timeout=30.0):
        deadline = time.time() + connect_timeout
        while True:
            try:
                self.s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                self.s.connect(path)
                break
            except OSError:
                if time.time() > deadline:
                    raise MonitorError("qemu monitor never came up at %s" % path)
                time.sleep(0.1)
        self.s.settimeout(5)
        time.sleep(0.4)
        self._drain()

    def _drain(self):
        out = b""
        try:
            while True:
                chunk = self.s.recv(65536)
                if not chunk:
                    break
                out += chunk
                if out.rstrip().endswith(b"(qemu)"):
                    break
        except socket.timeout:
            pass
        return out.decode("utf-8", "replace")

    def cmd(self, c, settle=0.25):
        self.s.sendall((c + "\n").encode())
        time.sleep(settle)
        return self._drain()

    def close(self):
        try:
            self.s.close()
        except OSError:
            pass


class Vm:
    """qemu guest.  Use as a context manager; the VM dies with the block."""

    def __init__(self, boot_img, gamedir, workdir, sock_dir="/tmp",
                 gdb_port=1234, memory_mb=16):
        self.boot_img = Path(boot_img).resolve()
        self.gamedir = Path(gamedir).resolve()
        self.workdir = Path(workdir).resolve()
        self.workdir.mkdir(parents=True, exist_ok=True)
        # 108-byte cap on AF_UNIX paths: never put the socket in the workdir.
        fd, self.sock = tempfile.mkstemp(prefix="rngtrace-", suffix=".sock", dir=sock_dir)
        os.close(fd)
        os.unlink(self.sock)
        if len(self.sock.encode()) > 100:
            raise MonitorError("monitor socket path too long: %s" % self.sock)
        self.gdb_port = gdb_port
        self.memory_mb = memory_mb
        self.proc = None
        self.mon = None

    # -- lifecycle -----------------------------------------------------
    def start(self):
        argv = [
            "qemu-system-i386",
            "-drive", "file=%s,format=raw,if=floppy" % self.boot_img,
            "-boot", "a",
            "-hda", "fat:rw:%s" % self.gamedir,
            "-display", "none", "-m", str(self.memory_mb),
            "-monitor", "unix:%s,server=on,wait=off" % self.sock,
            "-gdb", "tcp::%d" % self.gdb_port,
        ]
        self.qemu_log = open(self.workdir / "qemu.log", "w")
        self.proc = subprocess.Popen(argv, stdout=self.qemu_log,
                                     stderr=subprocess.STDOUT)
        self.mon = Monitor(self.sock)
        return self

    def kill(self):
        if self.mon is not None:
            self.mon.close()
            self.mon = None
        if self.proc is not None and self.proc.poll() is None:
            self.proc.kill()
            try:
                self.proc.wait(timeout=10)
            except subprocess.TimeoutExpired:
                pass
        self.proc = None
        if getattr(self, "qemu_log", None):
            self.qemu_log.close()
            self.qemu_log = None
        try:
            os.unlink(self.sock)
        except OSError:
            pass

    def __enter__(self):
        return self.start()

    def __exit__(self, *exc):
        self.kill()
        return False

    def alive(self):
        return self.proc is not None and self.proc.poll() is None

    # -- guest access --------------------------------------------------
    def pmemsave(self, addr, size, path):
        path = Path(path)
        if path.exists():
            path.unlink()
        out = self.mon.cmd('pmemsave 0x%x 0x%x "%s"' % (addr, size, path), settle=0.4)
        for _ in range(60):
            if path.exists() and path.stat().st_size == size:
                return path.read_bytes()
            time.sleep(0.1)
        raise MonitorError("pmemsave 0x%x+0x%x failed: %s" % (addr, size, out))

    def dump_memory(self, size=0x100000):
        return self.pmemsave(0, size, self.workdir / "mem.bin")

    def screen(self):
        """The 80x25 text screen, decoded from cp866 out of video RAM."""
        raw = self.pmemsave(0xB8000, 4000, self.workdir / "screen.bin")
        txt = bytes(raw[0::2]).decode("cp866", "replace")
        return "\n".join(txt[r * 80:(r + 1) * 80].rstrip() for r in range(25))

    def sendkey(self, name, settle=0.06):
        self.mon.cmd("sendkey " + name, settle=settle)

    def type(self, s, delay=0.06):
        for ch in s:
            self.sendkey(KEYMAP.get(ch, ch), settle=delay)

    def wait_for(self, needle, timeout=90, poll=1.0):
        end = time.time() + timeout
        last = ""
        while time.time() < end:
            if not self.alive():
                raise MonitorError("qemu died while waiting for %r" % needle)
            last = self.screen()
            if needle in last:
                return last
            time.sleep(poll)
        raise TimeoutError("never saw %r; last screen:\n%s" % (needle, last))
