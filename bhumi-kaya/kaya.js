// Bhumi Kaya - Tactile Interface Prototype
// Chording keyboard input -> 3x3 tactile grid output

// Valid chord keys
const LEFT_KEYS = ['f', 'd', 's', 'a'];
const RIGHT_KEYS = ['j', 'k', 'l', ';'];
const ALL_CHORD_KEYS = [...LEFT_KEYS, ...RIGHT_KEYS];

// Currently pressed keys
const pressedKeys = new Set();

// DOM elements
const cells = document.querySelectorAll('.cell');
const keyElements = document.querySelectorAll('.key');
const chordOutput = document.getElementById('chordOutput');
const message = document.getElementById('message');

// Convert chord to binary representation
// Left hand: f=8, d=4, s=2, a=1
// Right hand: j=8, k=4, l=2, ;=1
function chordToBinary(keys) {
    let left = 0;
    let right = 0;

    for (const key of keys) {
        const leftIdx = LEFT_KEYS.indexOf(key);
        if (leftIdx !== -1) {
            left |= (1 << (3 - leftIdx));
        }
        const rightIdx = RIGHT_KEYS.indexOf(key);
        if (rightIdx !== -1) {
            right |= (1 << (3 - rightIdx));
        }
    }

    return { left, right, combined: (left << 4) | right };
}

// Convert chord to a display string
function chordToString(keys) {
    const sorted = [...keys].sort((a, b) => {
        const order = [...LEFT_KEYS, ' ', ...RIGHT_KEYS];
        return order.indexOf(a) - order.indexOf(b);
    });
    return sorted.join('').toUpperCase().replace(' ', '_');
}

// Map chord patterns to 3x3 grid patterns
// Each pattern is an array of 9 booleans (or intensity values 0-1)
const CHORD_PATTERNS = {
    // Single keys - corners and edges
    'F': [1,0,0, 0,0,0, 0,0,0],
    'D': [0,1,0, 0,0,0, 0,0,0],
    'S': [0,0,1, 0,0,0, 0,0,0],
    'A': [0,0,0, 1,0,0, 0,0,0],
    'J': [0,0,0, 0,0,1, 0,0,0],
    'K': [0,0,0, 0,0,0, 1,0,0],
    'L': [0,0,0, 0,0,0, 0,1,0],
    ';': [0,0,0, 0,0,0, 0,0,1],

    // Two-key chords - lines and diagonals
    'FD': [1,1,0, 0,0,0, 0,0,0],
    'DS': [0,1,1, 0,0,0, 0,0,0],
    'FS': [1,0,1, 0,0,0, 0,0,0],
    'FA': [1,0,0, 1,0,0, 0,0,0],
    'DA': [0,1,0, 1,0,0, 0,0,0],
    'SA': [0,0,1, 1,0,0, 0,0,0],

    'JK': [0,0,0, 0,0,0, 1,1,0],
    'KL': [0,0,0, 0,0,0, 0,1,1],
    'JL': [0,0,0, 0,0,0, 1,0,1],
    'J;': [0,0,0, 0,0,1, 0,0,1],
    'K;': [0,0,0, 0,0,0, 1,0,1],
    'L;': [0,0,0, 0,0,0, 0,1,1],

    // Three-key chords - patterns
    'FDS': [1,1,1, 0,0,0, 0,0,0],
    'FDA': [1,1,0, 1,0,0, 0,0,0],
    'FSA': [1,0,1, 1,0,0, 0,0,0],
    'DSA': [0,1,1, 1,0,0, 0,0,0],

    'JKL': [0,0,0, 0,0,0, 1,1,1],
    'JK;': [0,0,0, 0,0,1, 1,1,0],
    'JL;': [0,0,0, 0,0,1, 1,0,1],
    'KL;': [0,0,0, 0,0,1, 0,1,1],

    // Four-key chords - full patterns
    'FDSA': [1,1,1, 1,0,0, 0,0,0],
    'JKL;': [0,0,0, 0,0,1, 1,1,1],

    // Cross-hand chords - center patterns
    'FJ': [1,0,0, 0,1,0, 0,0,1],
    'AJ': [0,0,0, 1,1,1, 0,0,0],
    'F;': [1,0,1, 0,1,0, 1,0,1],
    'A;': [0,0,0, 1,1,1, 0,0,0],

    // Center
    '_': [0,0,0, 0,1,0, 0,0,0],  // Space alone = center

    // Full grid
    'FDSA_JKL;': [1,1,1, 1,1,1, 1,1,1],
};

// Display pattern on the 3x3 grid
function displayPattern(pattern) {
    cells.forEach((cell, idx) => {
        const active = pattern[idx];
        cell.classList.toggle('active', active);
        if (active) {
            cell.classList.add('pulse');
            setTimeout(() => cell.classList.remove('pulse'), 300);
        }
    });
}

// Clear the grid
function clearGrid() {
    cells.forEach(cell => {
        cell.classList.remove('active', 'pulse');
    });
}

// Update key visual state
function updateKeyDisplay() {
    keyElements.forEach(el => {
        const key = el.dataset.key;
        el.classList.toggle('pressed', pressedKeys.has(key));
    });
}

// Get current chord string
function getCurrentChord() {
    const chordKeys = [...pressedKeys].filter(k => ALL_CHORD_KEYS.includes(k));
    const hasSpace = pressedKeys.has(' ');

    if (chordKeys.length === 0 && hasSpace) {
        return '_';
    }

    let str = chordToString(chordKeys);
    if (hasSpace && chordKeys.length > 0) {
        str += '_';
    }
    return str;
}

// Process the chord when space is released (or when chord is complete)
function processChord() {
    const chord = getCurrentChord();
    chordOutput.textContent = chord || 'Empty';

    // Look up pattern
    const pattern = CHORD_PATTERNS[chord];
    if (pattern) {
        displayPattern(pattern);
        message.textContent = `Pattern: ${chord}`;
    } else if (chord) {
        // Generate a pattern based on binary representation
        const chordKeys = [...pressedKeys].filter(k => ALL_CHORD_KEYS.includes(k));
        const { left, right, combined } = chordToBinary(chordKeys);
        message.textContent = `L:${left.toString(2).padStart(4,'0')} R:${right.toString(2).padStart(4,'0')}`;

        // Create pattern from binary - distribute across grid
        const generatedPattern = [
            (left & 8) ? 1 : 0, (left & 4) ? 1 : 0, (left & 2) ? 1 : 0,
            (left & 1) ? 1 : 0, pressedKeys.has(' ') ? 1 : 0, (right & 8) ? 1 : 0,
            (right & 4) ? 1 : 0, (right & 2) ? 1 : 0, (right & 1) ? 1 : 0,
        ];
        displayPattern(generatedPattern);
    } else {
        clearGrid();
        message.textContent = 'Press keys to create chords, space to send';
    }
}

// Keyboard event handlers
document.addEventListener('keydown', (e) => {
    const key = e.key.toLowerCase();

    // Only track our chord keys and space
    if (ALL_CHORD_KEYS.includes(key) || key === ' ') {
        e.preventDefault();

        if (!pressedKeys.has(key)) {
            pressedKeys.add(key);
            updateKeyDisplay();

            // Live preview of current chord
            const chord = getCurrentChord();
            chordOutput.textContent = chord || '...';
        }
    }
});

document.addEventListener('keyup', (e) => {
    const key = e.key.toLowerCase();

    if (pressedKeys.has(key)) {
        // If space is released, that's the "send" action
        if (key === ' ') {
            processChord();
        }

        pressedKeys.delete(key);
        updateKeyDisplay();

        // If all keys released, process whatever we had
        if (pressedKeys.size === 0) {
            // Small delay to allow for chord completion
            setTimeout(() => {
                if (pressedKeys.size === 0) {
                    clearGrid();
                    chordOutput.textContent = 'Ready';
                }
            }, 500);
        }
    }
});

// Prevent space from scrolling
window.addEventListener('keydown', (e) => {
    if (e.key === ' ') {
        e.preventDefault();
    }
});

// Initialize
console.log('Bhumi Kaya initialized');
console.log('Left hand: F D S A');
console.log('Right hand: J K L ;');
console.log('Space: Send/Confirm');
