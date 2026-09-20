//! The den's literal pool -- `1000:d802`..`1000:df01`.
//!
//! Every CS literal the den's span hands to `WriteLn` (`0eed:01c2`) or
//! `Write` (`0eed:0000`), in the image's own ADDRESS order, which is the
//! order `tools/difftest.py`'s `literal_walk` reads them in. The span ends
//! at `1000:df01` and not at the den's last instruction: `df01` pushes the
//! next verb's key literal, consumed by `1000:df06 call 0f78:0bd8`, and a
//! walk that included it would report a literal nothing in the span takes.
//!
//! `docs/re/port-gaps.md` recorded that the den had no oracle -- 44 branches
//! and not one `difftest` record scoped to its range. These tables are the
//! port half of that comparison; `difftest.py`'s `den(img)` derives the same
//! list straight out of `orig/g.exe` and the two are diffed record for
//! record.

/// `(closes, text)` -- `closes` is true for a `WriteLn` and false for a
/// `Write` with no newline behind it. Address order, not execution order.
pub const EMITTED: [(bool, &str); 31] = [
    (false, "Ты пришел в притон - "),                  // 1000:d816
    (true, "^0общагу №#"),                             // 1000:d836
    (true, "^0общагу ВКИ"),                            // 1000:d860
    (true, "^0гоповский притон"),                      // 1000:d880
    (true, "^0притон отморозков"),                     // 1000:d8a0
    (true, "^6На одного пацана наехал какой-то урод"), // 1000:d8cf
    (true, "^6Ты пацан нормальный. Есть дело."),       // 1000:d8f6
    (true, "^6Пацаны хотят тебе кое-чё сказать"),      // 1000:d943
    (true, "Напиши ^6w^7 чтобы уйти"),                 // 1000:d96b
    (
        true,
        "Напиши ^6hp^7 чтобы отпинать мудака который наезжал на пацана",
    ), // 1000:da3c
    (true, "Напиши ^6s^7  чтобы узнать отношение"),    // 1000:da55
    (true, "Напиши ^6a^7  чтобы спросить чё-то"),      // 1000:daa2
    (true, "Напиши ^6d^7 чтобы пойти на дело"),        // 1000:dac9
    (false, "^0Притон\\"),                             // 1000:dae2
    (
        true,
        "^2Ты угостил пацанов пивом. Понтовость улутшилась на 5.",
    ), // 1000:db43
    (true, "^6А нет у тебя пива."),                    // 1000:db5e
    (
        true,
        "^2Ты занял 2 рубля на пиво. Понтовость уменьшилась на 2.",
    ), // 1000:dba4
    (true, "^6Ты не можешь занять денег."),            // 1000:dbbf
    (true, "^6Ты уже всю мелочь выгреб!"),             // 1000:dbda
    (true, "^4Твоя понтовость сейчас = #."),           // 1000:dc74
    (true, "^0Да если чё мы за тебя впрягаемся."),     // 1000:dca1
    (true, "^0Тут у нас есть пара мест куда тебе стоит сходить"), // 1000:dd00
    (
        true,
        "^2Ты узнал где находится качалка и где находятся барыги",
    ), // 1000:dd19
    (true, "^0Давай быстрее.."),                       // 1000:dd5a
    (true, "^2Ты пришел воровать деньги"),             // 1000:dd73
    (true, "^4Шухер менты!"),                          // 1000:ddb6
    (true, "^6Пора валить!"),                          // 1000:ddff
    (true, "^2Ты смылся от ментов."),                  // 1000:de1a
    (true, "^2Ты наваровал денег"),                    // 1000:de36
    (true, "^6Ты получаешь # качков опыта"),           // 1000:de93
    (
        true,
        "^4Тебя мудака такого туда не пустят - поднимай понтовость",
    ), // 1000:dee3
];

/// Where the den's `WriteLn`s that carry no CS literal of their own fall
/// among [`EMITTED`]'s 31, as `(index, events)` -- the line sits BEFORE that
/// index. `B` is a bare `WriteLn` (a blank line), `C` one assembled on the
/// stack and printed whole.
///
/// **Read off the port, not off the image.** `Game::print_den_menu` prints
/// `term::println("")` before `EMITTED[5]` and again before `EMITTED[8]`,
/// then two `format!` lines before `EMITTED[9]`; the `s` arm's composed
/// `Это <имя> <N> уровня.` falls before `EMITTED[19]`. Generating this from
/// `orig/g.exe` would have made `difftest`'s comparison circular -- the
/// table and the reference would share a source and agree by construction.
/// Written this way the agreement is a finding: `gaps_of`'s sweep of
/// `1000:d802`..`df01` returns the same four.
///
/// The sweep also collects `ReadKey` (`0f16:031a`) as `K`, and there is not
/// one in the den. The Phase 3 audit established that by reading the span;
/// this table is what makes it COMPARED -- a `ReadKey` anywhere in the den
/// would put a `K` into the reference and this side would have to gain it.
pub const GAPS: &[(usize, &str)] = &[
    (5, "B"),  // 1000:d8be -- the blank above the three status lines
    (8, "B"),  // 1000:d961 -- the blank above `Напиши w чтобы уйти`
    (9, "CC"), // 1000:d9d4 and 1000:da30 -- the `p` and `r` menu rows
    (19, "C"), // 1000:dc53 -- the `s` arm's `Это <имя> <N> уровня.`
];

/// The CS literals the span hands to the string RTL (`0f78:0ae7` assign,
/// `0f78:0b66` append) rather than to a `Write`, in address order.
pub const FRAGMENTS: [&str; 6] = [
    "Напиши ^",                          // 1000:d99d
    "p^7  чтобы угостить пацанов пивом", // 1000:d9bb
    "Напиши ^",                          // 1000:d9f9
    "r^7  чтобы занять 2 рубля",         // 1000:da17
    "^6Это ",                            // 1000:dc1c
    " # уровня.",                        // 1000:dc39
];
