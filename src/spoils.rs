//! The victory block's literal pool -- `1000:523e`..`1000:57ce`.
//!
//! Everything `FUN_1000_3d11` prints after the XP award: the spoils
//! transfer, the post-kill one-shot rings, and the whole class-keyed item
//! table. In the image's ADDRESS order, which is the order
//! `tools/difftest.py`'s `literal_walk` reads them in.
//!
//! The span carries **no composed line, no fragment, no blank `WriteLn` and
//! no `ReadKey`** -- `gaps_of`'s sweep over it returns an empty table, and
//! that emptiness is itself compared: one `call 0f16:031a` anywhere in the
//! block would put a `K` into the reference and this side would have to
//! gain it.
//!
//! Four of these texts appear a second time in `src/game.rs`, in
//! `Game::church`'s arm 2. That is not duplication to fix: the original
//! holds two byte-identical copies of the ring block, at `1000:8101` and
//! `1000:532f` (`docs/re/progression.md` establishes they compare equal),
//! and only the `532f` one is in this span. The church's arm prints its own
//! copy and is not routed through this table.

/// `(closes, text)` -- `closes` is true for a `WriteLn`. Every literal in
/// the span closes its line; the tuple keeps the shape the other pools use
/// so `trace.rs` can emit them all the same way.
pub const EMITTED: [(bool, &str); 20] = [
    (true, "^1Пиво победителю!"), // 1000:5253, claim_spoils
    (
        true,
        "^1Поновость улутшилась на столько, что тебе можно заходить в местный притон!",
    ), // 1000:52b8, claim_spoils
    (true, "^1Оба на! Колечко! Вот свезло, так свезло!"), // 1000:52f6, grant_oneshot_gift
    (true, "^1Кольцо \"Помоги Господи\""), // 1000:5316, grant_oneshot_gift
    (true, "^1\"Мега Кольцо\"!"), // 1000:5371, grant_oneshot_gift
    (true, "^1Ваще полезное кольцо \"Господи помилуй\""), // 1000:53c0, grant_oneshot_gift
    (true, "^1Восст. жизни - 3, 5% - самозарост переломов"), // 1000:53d9, grant_oneshot_gift
    (true, "^1А у нарка был косячок"), // 1000:5430, claim_spoils
    (true, "^1Ты нашёл крестик: удача +2"), // 1000:5498, spoil_charm
    (true, "^1Ты нашёл кольцо \"Господи спаси\": удача +1"), // 1000:54c8, spoil_charm
    (true, "^1Ты нашёл мобилу"),  // 1000:54f4, spoil_charm
    (true, "^1Ты надыбал кастет(урон+2)"), // 1000:5546, spoil_club
    (true, "^6Но у тебя есть более мощное оружие"), // 1000:5580, spoil_club
    (true, "^1Ты отобрал у врага дубинку(урон+4)"), // 1000:55ac, spoil_club
    (true, "^6Но у тебя есть более мощное оружие"), // 1000:55f2, spoil_club
    (true, "^1Ты нашёл тёмные очки."), // 1000:562d, spoil_glasses
    (true, "^1Ты нашёл мобилу"),  // 1000:5654, spoil_glasses
    (true, "^1Ты нашел ножик(урон+6)."), // 1000:569d, spoil_blade
    (true, "^6Но утебя есть тесак который круче."), // 1000:5710, spoil_blade
    (true, "^1Ты нашел тесак(урон+9)!!! - ужасное оружие."), // 1000:5743, spoil_blade
];
