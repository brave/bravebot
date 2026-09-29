---
sidebar_position: 8
title: Editing the way vi does
description: Vi's INSERT, NORMAL, VISUAL and REPLACE modes in the input box, with its motions, counts, operators and text objects.
---

# Editing the way vi does

[`/config`](../reference/commands.md#config) chooses between the ordinary box and vi's editing keys.
The choice is written to `~/.bravebot`, so it outlives the session and applies in every directory,
and [`editorMode`](../customize/configuration.md#editormode) in a settings file answers for somebody
who has never made one.

Vi editing has two modes over the same line. **INSERT** is the box everybody has. **NORMAL** takes a
letter as an instruction, and a letter it has no instruction for does nothing at all rather than
being typed. NORMAL opens two more for a while, [VISUAL](#marking-a-stretch-out-first) and
[REPLACE](#typing-over-the-line). Every session opens in INSERT, whichever style is in force, and
the mode is drawn beneath the box beside the mode that says how much the session asks. The ordinary
box is in neither mode, and nothing about a mode is drawn at it.

An instruction still waiting for its next key is drawn after the mode, as vi's `showcmd` draws it:
`NORMAL d` once `d` is pressed, `NORMAL di` after the `i`, and the mode alone once the instruction is
whole.

A key beginning one of vi's instructions that this box does not have does nothing, and nor does the
key vi would give it: `"`, `q`, `@`, `m`, `'`, `` ` ``, `z`, `Z`, `[`, `]`, `g'` and `` g` ``
each take one more key, so `ma` sets no mark and opens no INSERT mode. `g?`, `gq`, `gw` and `g@`
take the stretch they would act on, so `g?iw` changes nothing. After an operator only `'`, `` ` ``, `[`, `]` and `z` take their key, as in vi, so
`dm` ends the `d` and the key after it is read on its own. In VISUAL mode those four take no key,
since the selection is the stretch, and `R` changes the rows the selection crosses.

**Escape enters NORMAL mode and leaves the line exactly as it was.** It also abandons an instruction
still waiting for a key, so `d`, Escape, `w` moves a word rather than deleting one, and so does any
other key that is not a character, such as an arrow or Enter. Discarding a
half-typed line is still Ctrl-C. Ctrl-`[` is the same request from a terminal that reports the
modifier rather than sending the byte Escape already is. In the ordinary box Escape discards the line
as it always has.

**While a turn runs, Escape stops it only once the box has no use for the press:**

| The box | Escape |
|---|---|
| INSERT, VISUAL or REPLACE | enters NORMAL mode, and the turn keeps running |
| NORMAL, an instruction or a count waiting | abandons it, and the turn keeps running |
| NORMAL, nothing waiting | stops the turn |

So the Escape you press out of habit after sending leaves INSERT, and a second one stops the turn.
Ctrl-C stops the turn on the first press from any mode. A summary, an aside, a goal check and a
manifest run take Escape the same way. A shell command takes no keys at the box, so
Escape stops one from any mode. In the ordinary box Escape stops a turn on the first press.

## Getting back into INSERT mode

| Key | Where the caret lands |
|---|---|
| `i`, `a` | before or after the character the caret is on |
| `I`, `A` | the first character of the line, the end of the line |
| `o`, `O` | a new line below, a new line above |

Leaving INSERT mode puts the caret on a character rather than past the end of the line, since in
NORMAL mode it sits on the character the next instruction acts on.

`!` and `?` are instructions in NORMAL mode rather than the marks that arm
[shell mode](shell-mode.md) and put the key list up. Both are a press of `i` away. Reading `!` as the
mark would arm a shell from a press asking for something else, and there is no way out of a shell
armed by accident except deleting back past the mark.

## Typing over the line

`R` opens REPLACE mode, and the hint line says `REPLACE`. Each character you type takes the place of
the one under the caret, until Escape takes you back to NORMAL mode. At the end of a row and on a
[marker](./interactive-mode.md#markers) the character goes in beside the caret instead, and Shift-Enter breaks the row
without taking anything; Enter sends. Backspace puts back what the last character you typed took the
place of, as far back as the `R`, and past that only moves the caret left. Ctrl-W and Ctrl-U do the
same as far as the start of the word and of the row. `u` takes back everything typed since the `R` as
one change. `.` does not repeat it, and a count in front of `R` does nothing.

## Motions

| Keys | Where the caret goes |
|---|---|
| `h`, `l`, Space | one character left or right |
| `w`, `e`, `b`, `ge` | the start of the next word, the end of this word or the next, the start of this word or the previous, the end of the word before |
| `W`, `E`, `B`, `gE` | the same four, where a word is a run of anything that is not a blank |
| `0`, `$`, `^` | the first column, the last character, the first character that is not a blank |
| `_` | the first character that is not a blank, on the row a count names counting this one as the first |
| `\|` | the column a count names, counting the first as one |
| `gg`, `G` | the first character that is not a blank on the first line of the input, and on the last |
| `%` | the bracket that pairs with the first one at or after the caret on this line |
| `f`, `F`, `t`, `T` then a character | the next or previous occurrence of it on this line, landing on it or stopping one short |
| `;`, `,` | that jump again, and the same jump reversed |

A jump looks only along the line the caret is on, and one that finds nothing leaves the caret where
it was. `w` lands on the first character of the next word, rather than after the word it crossed
where the word keys under Ctrl land.

**`%` stays on the caret's line, where vi's crosses lines.** It pairs `(` with `)`, `[` with `]` and
`{` with `}`, counting only brackets of the kind it found, and a bracket with no partner on the line
leaves the caret where it was. A pair split over rows, a block pasted in with its closing brace three
rows down, is `j` and `f}` away instead. The brackets a picture or paste is written with are not
brackets to it. A count in front of `%` is spent and moves nothing.

**`w` ends a word where punctuation begins, as vi does.** In `src/main.rs` each name is a word and so
are the slash and the dot, so `dw` on `src` takes `src` alone. `W` crosses the whole path in one
press. A picture or paste is a word by itself to `w`, `e`, `b` and `ge`, and part of the word it
touches to the capitals. An empty row is a stop for `w`, `b` and `ge`, so none of them crosses a
paragraph break in one press.

**`k`, `j` and `/` are the keys they spell rather than motions of their own.** `k` and `j` are Up and
Down: they walk the rows of a paragraph, then your prompt history, then the transcript, exactly as
the arrows do. `/` opens the search Ctrl-R opens, that being the only search here. While a key is
waiting for the character to jump to, every press is that character, so `f/` jumps to a slash and
`fj` to a `j`. A count in front of `k` or `j` is the other exception, and moves rows inside the
input alone.

## Counts

A number in front of an instruction says how many. `1` to `9` begin a count and every digit after
one continues it, so `0` is still the key for the first column and `10l` is ten characters. A count
in front of an operator and one in front of its motion multiply, so `2d3w` is `d6w`.

| Keys | What the count says |
|---|---|
| `3w`, `5l`, `2f,`, `3;` | how many times over the motion is meant |
| `2G`, `2gg`, `2_` | which row to go to, where `_` counts the caret's row as the first |
| `8\|` | which column to go to |
| `3j`, `3k` | how many rows to move, inside the input and no further |
| `3dd`, `d3w`, `3x`, `3X`, `3~`, `3rx`, `3gUU`, `3>>` | how much of the stretch the operator takes |
| `3p`, `3J`, `3gJ` | how many copies go back, and how many rows end up as one |
| `3.` | how many the repeat is of, in place of the count it recorded |
| `3u` | how many changes to take back |

**The line bounds a count, not the number you type.** A counted motion stops at the first step that
moves nothing, so `999l` reaches the end of the line, and an extent takes what there is, so `9dd` on
a two-row paragraph takes the two rows. `p` is the exception, since a copy always goes somewhere: it
puts back as many as you asked for. `r` takes all of its count or nothing, so `5rx` with two
characters left changes neither. A counted change is one change and one step to undo.

The count is drawn after the mode with the rest of the instruction, so three presses are
`NORMAL 2d3`. Escape abandons it, as it abandons any instruction still waiting for a key.

## Operators

`d` takes a stretch out, `c` takes it out and opens INSERT mode where it was, `y` keeps it and leaves
the line alone, `>` and `<` move every row the stretch reaches a step from or towards the margin, and
`gu`, `gU` and `g~` make it lower case, upper case or the other case. Each waits for the stretch to
act on:

| Keys | The stretch |
|---|---|
| a motion | from the caret to wherever that motion would take it |
| `j`, `k`, `G`, `gg`, `_` | every row from the caret's to the one the key reaches, whole |
| the operator's own letter doubled | the whole line: `dd`, and `guu` or `gugu` |
| `D`, `C`, `x`, `s` | to the end of the line, and the character under the caret |
| `X` | the character before the caret |
| `Y`, `S` | the whole line |

`~` changes the case of the character under the caret and moves on, so pressing it again walks
along the line. `r` or `gr` then a key makes the character under the caret that key, and `3rx` makes
three of them `x`. `r` then Enter is the Enter alone, where vi would break the line, since Enter
abandons a key still waiting. Case changes as vim changes it: `gUiw` over `Straße` is `STRASSE`, and
a letter with no capital of its own, such as the `ﬀ` ligature, stays as it is.

So `dw`, `cw` and `yw` are one idea rather than three bindings, and `d$` and `dG` work without being
listed. Whether the character a motion landed on is taken depends on the motion, as it does in vi:
`de` takes the word's last letter where `dw` stops before the next word's first, and `cw` on a
character that is not a blank behaves as `ce`, leaving the space after the word. `cW` is `cE` in the
same way, and on the last character of a word either changes that character alone. `dw` on the last
word of a row takes the word and leaves the newline. `dge` takes both ends: the last letter of the
word before, and the character the caret was on. `d%` takes both brackets and everything between
them, from either bracket, and `d|` stops short of the column, so `d|` is `d0`. `d_` is `dd`, and
`d3_` is `3dd`.

`p` and `P` put the register back after and before the caret, and a stretch that was whole lines comes
back as a line of its own. `J` makes this line and the one below into one, with a single space where
the newline was. `gJ` joins them with nothing there and leaves the blanks the next line began with,
for a line broken in the middle of a word or a path.

`u` puts back what the last change took, and pressed again the change before that, **as far back as
a thousand changes**, the same depth as vim's. Everything typed in INSERT mode after `c` or one of
the keys that open it is one change with the key, so one `u` takes back `o`'s new row and all you
typed on it. Sending, clearing or putting away the line leaves nothing to undo, and so does a line
the box is handed whole, such as a recalled prompt. There is no redo: Ctrl-R is the prompt search.

`.` does the last change again at the caret. After `c`, `s`, `S`, `C`, `i`, `a`, `I`, `A`, `o` or
`O` that includes what you typed, so `cw`, `X`, Escape, `w`, `.` changes the next word to `X` as
well, and `p`, `P`, `J` and `gJ` are made again too. It does nothing after typing that attached
something, whether a picture, a paste the box folded into a marker or an `@` name, since doing it
again would attach the same thing twice, and nothing after a completion you chose from a list. It
does nothing either after a put, a join, `r` or a case change over a selection, `d` or `c` over a
selection made with `v` rather than `V`, or typing over the line with `R`.

The register is vi's unnamed one and the only one. It is not the system clipboard, which Ctrl-V owns
and which you share with every other window you have open, so a yank here does not travel out of the
box. Only `d`, `c`, `y`, the keys spelled from them and `p` over a selection fill it: a shift, a case
change and `r` leave what you yanked there to put back.

## Text objects

After an operator, `i` and `a` say the stretch is a thing rather than a distance, and the next press
says which: `w` a word, `W` a run of anything that is not a blank, and a quote or either half of a
bracket pair for what lies between them, with `b` for the round pair and `B` for the curly one, as in
vi. `i` takes what is inside and `a` takes what surrounds it too. All on the line the caret is on.

`ci(` is what you mean when you want the arguments replaced, and it works with the caret on the name
in front of the bracket, where it usually is: a pair is the one enclosing the caret, or else the next
one along the line. Either half names it, so `di(` and `di)` are the same request.

`w` treats punctuation as a word of its own, so in `src/main.rs` the slashes belong to neither name.
`W` is the same thing with punctuation folded in, which is why `aW` takes the whole path.

## Marking a stretch out first

`v` marks a stretch out character-wise and `V` line-wise, and **the whole of it is drawn while you
choose it**, on every row it crosses. That is the point of having both this and an operator waiting
for a motion: the stretch is on the screen while it is being chosen, and the next key acts on it.

An operator here needs no extent: `x` is `d` and `s` is `c`, `r` replaces every selected character
with one, and `~`, `u` and `U` change the case, as do `g~`, `gu` and `gU`. The capitals act on every
row the selection crosses, whole: `D` and `X` take them, `Y` keeps them, and `C`, `S` and `R` change
them. After any of those but `Y`, or after `d`, `c`, `>` or `<` over a line-wise selection, `.` does
it again to as many rows from the caret. Motions move the end the caret is at, and a count moves it
further, so `v2j` marks three rows. `o` puts the caret at the other end, and a text object becomes
the selection.

`p` puts what you yanked where the selection is and keeps what the selection held, so pressing `p`
over another stretch swaps the two. `P` does the same and keeps what you yanked, to put one yank over
several stretches. A yanked row stays a row, splitting the line when you put it over part of one.

The key that opened the mode closes it, the other of the two changes which kind is in force, and
Escape abandons the selection. Every operator ends it, so nothing acts on a stretch that is no longer
drawn.

`gv` marks the last selection out again, however it ended, with each end at the row and the column it
was at, so `V>` and then `gv>` shifts the same rows twice. In VISUAL mode `gv` swaps the selection on
the screen for the one before.

A selection is character-wise or line-wise and never a rectangle. A box ten rows tall holding one
prompt is not where somebody edits columns.

## These keys and a marker

A [marker](./interactive-mode.md#markers) is one thing to every key above. A motion crosses it whole and leaves the caret
nowhere inside it. An operator takes it whole or not at all, and taking it takes the attachment off.
`r` over one does nothing, over a selection or not: replacing the text either side and leaving the
marker standing would be a line nobody could read. A case change goes around one and leaves it as it
was, since a marker spelled in capitals would name no picture.
