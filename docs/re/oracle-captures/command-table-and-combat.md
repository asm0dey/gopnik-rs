# Live oracle captures: command table and combat modality

> **These captures are EVIDENCE, not specification.** The command list below is
> a string the game prints; it is not the code that parses input. A printed help
> text can be stale or incomplete relative to its dispatcher, and this binary
> already ships typos and inconsistencies. The authority for the command table
> is the input-dispatch chain in the disassembly. Use these screens to
> corroborate a branch you found in code, never to stand in for finding it.
> Where a capture and the dispatcher disagree, that disagreement is a finding.

Captured from `orig/g.exe` under DOSBox-X via `tools/oracle/capture.py`,
seed pinned to 12345. These are screens the ORIGINAL printed, not the port.

## The in-game command list (district 1), printed by typing `i` at the `\` prompt

```
\i
Напиши: w    чтобы шататься по окрестностям - искать на свою жопу приключения
Напиши: mar  чтобы идти на рынок
Напиши: rep  чтобы идти к ветеринару
Напиши: pr   чтобы идти в местный притон гопоты
Напиши: s    чтобы посмотреть в лужу на свою уродскую рожу
Напиши: sv   чтобы приглядеться к пинаемому мудаку
Напиши: k    чтобы гасить мудака который тебе попался на дороге
Напиши: v    чтобы позвать подкрепление
Напиши: kos  чтобы схавать косяк
Напиши: h    чтобы выпить пиво (если не охото к ветеринару)
Напиши: mh   чтобы набухаться до чёртиков
Напиши: name чтобы сменить погоняло
Напиши: e    если захочешь выйти
\
```

Keys used: ` 1
<20 spaces>2
Vasya
i
`

## Combat is modal: location verbs are ignored at the `Битва\` prompt

```
w
Ты смылся.
\w
Ничё не происходит.
\w
Ничё не происходит.
\w
Идет Беспредельщик 0 уровня, ищущий кого отпинать. Хочешь наехать?
w
Он тебя заметил.
Эй мудак?!
Битва\w
Битва\w
Битва\w
Битва\mar
Начинают собираться зрители
Битва\mar
Битва\mar
Битва\mar
Зрители:Врежь ему!
Битва\i
Битва\
```

Keys used: ` 1
<20 spaces>2
Vasya
` then `w
` x11, `mar
` x4, `i
`
