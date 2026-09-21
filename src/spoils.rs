//! Text printed after the XP award: the spoils transfer, the post-kill
//! one-shot rings, and the whole class-keyed item table.
//!
//! Four of these texts also appear in [`crate::game::Game::church`]'s arm 2.
//! That is not duplication to fix: the original prints its own copy there,
//! independent of this table.

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
