; SCRHOOK.COM -- oracle recorder/keyboard driver, resident under DOSBox-X.
;
; Hooks INT 16h. On every *blocking* key read (AH=00h/10h) it
;   1. appends the 80x25 text screen (B800:0000, 4000 bytes) to SCREEN.BIN
;   2. returns the next byte of the KEYS.TXT script instead of a real key.
;
; Both halves are pinned to game state rather than to wall-clock time: one
; frame per key the game asks for, and the Nth request always gets the Nth
; scripted key no matter how fast the emulator runs. Once the script is
; exhausted the handler chains to the BIOS, so the game simply waits and the
; host tears the emulator down.
;
; A dump calls DOS from inside an interrupt handler, so it checks the InDOS
; flag first and does nothing while DOS is busy (COMMAND.COM's line input
; reaches INT 16h from inside INT 21h), and it makes its own PSP current
; around the write because file handles are indexed per PSP.
;
; Key encoding in KEYS.TXT: one byte per keystroke, returned in AL with
; AH=0. A 00h byte is an escape: the following byte is returned in AH with
; AL=0, which is how the BIOS reports extended keys (arrows, function keys).

        org     0x100
        bits    16

KEYBUF_SIZE     equ     1024

start:  jmp     install

; ---- resident data -------------------------------------------------------
oldvec: dd      0               ; previous INT 16h handler
indos:  dd      0               ; far pointer to the DOS InDOS flag
fh:     dw      0               ; SCREEN.BIN file handle
mypsp:  dw      0               ; our PSP segment
kptr:   dw      keybuf          ; next unread key in keybuf
kend:   dw      keybuf          ; one past the last key
served: dw      0               ; key to hand back, loaded after popa

; ---- resident code -------------------------------------------------------
isr:
        cmp     ah, 0x00        ; read key (blocking)
        je      .serve
        cmp     ah, 0x10        ; read key, extended (blocking)
        je      .serve
        jmp     far [cs:oldvec]
.serve:
        pusha
        push    ds
        push    es
        les     bx, [cs:indos]
        cmp     byte [es:bx], 0 ; DOS busy? then neither dump nor inject
        jne     .chain

        mov     ah, 0x51        ; current PSP -> BX
        int     0x21
        push    bx
        mov     bx, [cs:mypsp]
        mov     ah, 0x50        ; make our PSP current for the write
        int     0x21
        mov     bx, [cs:fh]
        mov     ax, 0xB800
        mov     ds, ax
        xor     dx, dx
        mov     cx, 4000
        mov     ah, 0x40        ; append this screen to SCREEN.BIN
        int     0x21
        mov     bx, [cs:fh]
        mov     ah, 0x68        ; commit, so the host sees the frame at once
        int     0x21
        pop     bx
        mov     ah, 0x50        ; restore the interrupted program's PSP
        int     0x21

        mov     si, [cs:kptr]   ; script exhausted -> let the BIOS answer
        cmp     si, [cs:kend]
        jae     .chain
        mov     al, [cs:si]
        inc     si
        xor     ah, ah
        test    al, al          ; 00h escape: next byte is a scan code
        jnz     .got
        cmp     si, [cs:kend]
        jae     .chain
        mov     ah, [cs:si]
        inc     si
.got:
        mov     [cs:kptr], si
        mov     [cs:served], ax
        pop     es
        pop     ds
        popa
        mov     ax, [cs:served]
        iret
.chain:
        pop     es
        pop     ds
        popa
        jmp     far [cs:oldvec]

keybuf: times KEYBUF_SIZE db 0
resident_end:

; ---- install (discarded once resident) -----------------------------------
install:
        mov     ax, cs          ; COM: CS = DS = our PSP segment
        mov     [mypsp], ax

        mov     ah, 0x3c        ; create SCREEN.BIN
        xor     cx, cx
        mov     dx, fnscreen
        int     0x21
        jc      fail
        mov     [fh], ax

        mov     ax, 0x3d00      ; open KEYS.TXT for reading
        mov     dx, fnkeys
        int     0x21
        jc      fail
        mov     bx, ax
        mov     ah, 0x3f        ; read the whole script into keybuf
        mov     cx, KEYBUF_SIZE
        mov     dx, keybuf
        int     0x21
        jc      fail
        add     ax, keybuf
        mov     [kend], ax
        mov     ah, 0x3e        ; close KEYS.TXT
        int     0x21

        mov     ah, 0x34        ; InDOS flag address -> ES:BX
        int     0x21
        mov     [indos], bx
        mov     [indos+2], es

        mov     ax, 0x3516      ; current INT 16h vector -> ES:BX
        int     0x21
        mov     [oldvec], bx
        mov     [oldvec+2], es

        mov     ax, 0x2516      ; install our handler
        mov     dx, isr
        int     0x21

        mov     dx, (resident_end - start + 0x10f) / 16
        mov     ax, 0x3100      ; terminate and stay resident
        int     0x21
fail:
        mov     ax, 0x4c01
        int     0x21

fnscreen:       db      'SCREEN.BIN', 0
fnkeys:         db      'KEYS.TXT', 0
