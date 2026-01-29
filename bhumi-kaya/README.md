# Bhumi Kaya

A tactile computing interface - an alternative to screen/keyboard/mouse based
computing.

## Vision

Traditional computing interfaces rely on visual output (screens) and manual
input (keyboards, mice, touchscreens). Bhumi Kaya explores a different modality:
**tactile I/O** - communicating with computers through touch sensations on skin.

The goal is to build an "operating system" where:

- **Output** is felt, not seen - patterns of pressure/vibration on a 3x3 grid
  worn on the body
- **Input** is chorded, not typed - simultaneous key combinations rather than
  sequential keypresses

This enables computing that doesn't require eyes or ears - useful for
accessibility, eyes-free operation, or simply a more intimate human-computer
interaction.

## The Hardware (Future)

**Bhumi Kaya** (Sanskrit: भूमि काया, "earth body") is a wearable wrap-around
cloth/velcro band with:

- 9 tactile actuators in a 3x3 grid (output)
- Conductive fabric touch points for chording input

For now, we prototype in the browser with keyboard input and visual grid output.

## The Interface

### Input: Chording Keyboard

Instead of pressing keys one at a time, chording means pressing multiple keys
simultaneously to create a single input.

```
Left Hand (home row):    F  D  S  A
Right Hand (home row):   J  K  L  ;

Space: Send / Mode switch / Confirm
```

With 8 keys and space, we can create:

- 4 single-key inputs per hand (8 total)
- 6 two-key chords per hand (12 total)
- 4 three-key chords per hand (8 total)
- 1 four-key chord per hand (2 total)
- Plus cross-hand combinations
- Plus space as modifier

This gives us hundreds of distinct chord combinations - enough for a full
alphabet, numbers, commands, and navigation.

### Output: 3x3 Tactile Grid

```
┌───┬───┬───┐
│ 1 │ 2 │ 3 │
├───┼───┼───┤
│ 4 │ 5 │ 6 │
├───┼───┼───┤
│ 7 │ 8 │ 9 │
└───┴───┴───┘
```

9 points of contact, each capable of:

- On/off binary state
- Variable intensity (future)
- Vibration patterns (future)
- Temporal sequences

With 9 binary points, we have 512 possible patterns. Combined with
timing/sequences, the bandwidth is sufficient for:

- Alphabet (26 letters)
- Numbers (10 digits)
- Navigation feedback (directions, selections)
- Status indicators
- Even simple "graphics" through pattern sequences

## Design Principles

1. **Minimal but sufficient** - 3x3 is the smallest grid that has a center,
   edges, and corners. 8 chord keys is the maximum comfortable for two hands on
   home row.
2. **Learnable patterns** - Mappings should be memorable. Letters might map to
   patterns that "feel" like the letter shape. Navigation uses spatial patterns
   (left edge = go left).
3. **Two-way communication** - Both input and output use similar mental models.
   The 8 input keys can conceptually map to the 8 non-center cells.
4. **Eyes-free operation** - The entire interaction should work without looking
   at anything.

## Running the Prototype

```bash
cd bhumi-kaya
python3 -m http.server 8080
# Open http://localhost:8080
```

Or simply open `index.html` in a browser.

## The Larger Vision

Bhumi Kaya is part of the larger Bhumi ecosystem - decentralized, peer-to-peer
IoT. Imagine:

- A wearable that receives notifications as tactile patterns - you "feel" who's
  calling
- Navigation directions felt on your arm while cycling
- Silent, private communication between two people wearing Kayas
- Accessibility computing for visually impaired users
- Meditation/focus applications with biofeedback

The skin is our largest organ. It's time we used it for computing.

## Etymology

- **Bhumi** (भूमि): Earth, ground, foundation
- **Kaya** (काया): Body, embodiment

Together: "Embodied foundation" - computing that is grounded in the physical
body rather than abstracted onto screens.
