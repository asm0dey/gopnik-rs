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
    (true, "^1Пиво победителю!"), // Claim_spoils
    (
        true,
        "^1Поновость улутшилась на столько, что тебе можно заходить в местный притон!",
    ), // Claim_spoils
    (true, "^1Оба на! Колечко! Вот свезло, так свезло!"), // Grant_oneshot_gift
    (true, "^1Кольцо \"Помоги Господи\""), // Grant_oneshot_gift
    (true, "^1\"Мега Кольцо\"!"), // Grant_oneshot_gift
    (true, "^1Ваще полезное кольцо \"Господи помилуй\""), // Grant_oneshot_gift
    (true, "^1Восст. жизни - 3, 5% - самозарост переломов"), // Grant_oneshot_gift
    (true, "^1А у нарка был косячок"), // Claim_spoils
    (true, "^1Ты нашёл крестик: удача +2"), // Spoil_charm
    (true, "^1Ты нашёл кольцо \"Господи спаси\": удача +1"), // Spoil_charm
    (true, "^1Ты нашёл мобилу"),  // Spoil_charm
    (true, "^1Ты надыбал кастет(урон+2)"), // Spoil_club
    (true, "^6Но у тебя есть более мощное оружие"), // Spoil_club
    (true, "^1Ты отобрал у врага дубинку(урон+4)"), // Spoil_club
    (true, "^6Но у тебя есть более мощное оружие"), // Spoil_club
    (true, "^1Ты нашёл тёмные очки."), // Spoil_glasses
    (true, "^1Ты нашёл мобилу"),  // Spoil_glasses
    (true, "^1Ты нашел ножик(урон+6)."), // Spoil_blade
    (true, "^6Но утебя есть тесак который круче."), // Spoil_blade
    (true, "^1Ты нашел тесак(урон+9)!!! - ужасное оружие."), // Spoil_blade
];
