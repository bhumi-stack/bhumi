# Bhumi Kaya Grammar

A tactile computing interface using chording input and Braille output.

## Input: Chords

**Keys** (use one hand, left or right, always 1-2-3-4 left to right):
```
Left:   A  S  D  F     Right:  J  K  L  ;
        1  2  3  4             1  2  3  4
```

**Chord notation**: digits combined, e.g., `14` = keys 1+4 pressed together

**Possible chords** (15 total):
- Single: `1` `2` `3` `4`
- Double: `12` `13` `14` `23` `24` `34`
- Triple: `123` `124` `134` `234`
- Quad:   `1234`

## Output: Pratyaya (⠀-⣿)

**Braille 8-dot layout**:
```
1 4
2 5
3 6
7 8
```

**Pattern notation**: dot numbers, e.g., `1478` = dots 1,4,7,8 = ⣉

**Unicode range**: U+2800 (⠀ empty) to U+28FF (⣿ full)

## Kriya: Actions

**Syntax**: `[chord] [chord]... __`

- Chord keys form a chord
- Space (`_`) finalizes chord and adds to sequence
- Space with no chord = execute the sequence
- Two spaces (`__`) = execute

**Examples**:
```
14__        → execute kriya "14"
14 12__     → execute kriya "14_12"
__          → execute kriya "" (empty)
```

## Global Kriyas

| Input      | Kriya Key | Action      | Pratyaya Response     |
|------------|-----------|-------------|-----------------------|
| `__`       | ``        | replay      | (last sequence)       |
| `13__`     | `13`      | get time    | HH:MM as digit seq    |
| `14__`     | `14`      | query mode  | mode pratyaya         |
| `14 [m]__` | `14_[m]`  | switch mode | mode pratyaya         |
| `1234__`   | `1234`    | clear input | (none)                |

## Global Pratyayas

| Pattern | Braille | Dots     | Meaning   |
|---------|---------|----------|-----------|
| ⠀       | U+2800  | (none)   | empty     |
| ⣿       | U+28FF  | 12345678 | all/full  |
| ⠊       | U+280A  | 24       | cancelled |
| ⣉       | U+28C9  | 1478     | error     |
| ⠒       | U+2812  | 25       | confirm/: |

## Digit Pratyayas

Standard Braille number patterns (0-9) plus custom patterns for 10-12.
Dots 7,8 (bottom row) serve as a "tens" indicator.

| Digit | Pattern | Dots   |
|-------|---------|--------|
| 0     | ⠚       | 245    |
| 1     | ⠁       | 1      |
| 2     | ⠃       | 12     |
| 3     | ⠉       | 14     |
| 4     | ⠙       | 145    |
| 5     | ⠑       | 15     |
| 6     | ⠋       | 124    |
| 7     | ⠛       | 1245   |
| 8     | ⠓       | 125    |
| 9     | ⠊       | 24     |
| 10    | ⣀       | 78     |
| 11    | ⣁       | 1,78   |
| 12    | ⣃       | 12,78  |

## Modes

Each mode has:
- **Code**: chord pattern (e.g., `1`, `12`)
- **Name**: human readable
- **Pratyaya**: signature pattern
- **Kriyas**: mode-specific actions
- **Pratyayas**: mode-specific pattern meanings

### Mode 1: Default
- Code: `1`
- Pratyaya: ⠁ (dot 1)

### Mode 12: (example)
- Code: `12`
- Pratyaya: ⠃ (dots 1,2)

## Timing

- **Auto-cancel**: 2 seconds of no input
- **Cancelled pratyaya**: ⠊⠊ (24-24)

## Reading Mode

Some kriyas return multiple pratyayas (e.g., time). These enter **reading mode**:

- **▼ indicator**: pulsing arrow shows more content available
- **Space**: advances to next pratyaya
- **Any chord**: cancels reading mode

Reading mode allows user-paced consumption of pratyaya sequences.

## Sequences

Pratyayas can be sequences (up to 3 patterns):
```
⣿ → ⠀ → ⣉⣉     (all, pause, error-error)
```

## Grammar Summary

```
session     := kriya*
kriya       := chord* "__"
chord       := key+ "_"
key         := "1" | "2" | "3" | "4"

response    := pratyaya_seq
pratyaya_seq := pratyaya+
pratyaya    := braille_char
braille_char := U+2800..U+28FF
```

## Examples

**Query current mode**:
```
Input:  14__
Output: ⠒⠀  (confirm pattern for default mode, then clear)
```

**Switch to mode 12**:
```
Input:  14 12__
Output: ⠃⠀  (mode 12 pattern, then clear)
```

**Get current time** (e.g., 2:37 PM):
```
Input:  13__
Output: ⠃ (space) ⠉ (space) ⠛ (space) ⠀
        2         3         7         done
        hour      min-tens  min-unit
```

**Get current time** (e.g., 1:11 PM):
```
Input:  13__
Output: ⠁ (space) ⣁ (space) ⠀
        1         11        done
        hour      minutes
```

Time format: 12-hour clock
- Minutes 0-12: shown as single pattern (2 total)
- Minutes 13-59: shown as tens + units (3 total)

**Unknown kriya**:
```
Input:  23__
Output: ⣉⣉⠀  (error-error, then clear)
```

**Timeout cancel**:
```
Input:  14 (wait 2s)
Output: ⠊⠊⠀  (cancelled-cancelled, then clear)
```

**Replay last**:
```
Input:  __
Output: (repeats last pratyaya sequence)
```

**Clear input**:
```
Input:  14 (mistake!) 1234__
Output: (cleared, ready)
```
