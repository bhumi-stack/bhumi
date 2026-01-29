// Bhumi Kaya - Tactile Interface Prototype
// Keys: 1=F/J, 2=D/K, 3=S/L, 4=A/;
// Pratyaya: 2x4 Braille (8 dots, 256 patterns)

const LEFT_KEYS = { 'a': '1', 's': '2', 'd': '3', 'f': '4' };
const RIGHT_KEYS = { 'j': '1', 'k': '2', 'l': '3', ';': '4' };
const NUM_KEYS = { '1': '1', '2': '2', '3': '3', '4': '4' };
const KEY_TO_NUM = { ...LEFT_KEYS, ...RIGHT_KEYS, ...NUM_KEYS };

const AUTO_CANCEL_MS = 2000;

// Braille: dots 1-8, layout: 1 4 / 2 5 / 3 6 / 7 8
function patternFromDots(...dots) {
    const p = [0, 0, 0, 0, 0, 0, 0, 0];
    dots.forEach(d => { if (d >= 1 && d <= 8) p[d - 1] = 1; });
    return p;
}

function patternToBraille(pattern) {
    let code = 0x2800;
    [1, 2, 4, 8, 16, 32, 64, 128].forEach((bit, i) => {
        if (pattern[i]) code += bit;
    });
    return String.fromCharCode(code);
}

// Global pratyayas (recognized patterns)
const globalPratyayas = {
    '24': { label: 'cancelled', pattern: patternFromDots(2, 4) },
    '1478': { label: 'error', pattern: patternFromDots(1, 4, 7, 8) },
    '12345678': { label: 'all', pattern: patternFromDots(1, 2, 3, 4, 5, 6, 7, 8) },
    '25': { label: 'confirm', pattern: patternFromDots(2, 5) },
};

const PRATYAYA = {
    empty: patternFromDots(),
    all: patternFromDots(1, 2, 3, 4, 5, 6, 7, 8),
    cancelled: patternFromDots(2, 4),
    error: patternFromDots(1, 4, 7, 8),
    confirm: patternFromDots(2, 5),
    colon: patternFromDots(2, 5),  // separator for time
};

// Braille digit patterns (standard braille numbers + custom 10-12)
const DIGIT_PRATYAYA = [
    patternFromDots(2, 4, 5),     // 0 = ⠚
    patternFromDots(1),           // 1 = ⠁
    patternFromDots(1, 2),        // 2 = ⠃
    patternFromDots(1, 4),        // 3 = ⠉
    patternFromDots(1, 4, 5),     // 4 = ⠙
    patternFromDots(1, 5),        // 5 = ⠑
    patternFromDots(1, 2, 4),     // 6 = ⠋
    patternFromDots(1, 2, 4, 5),  // 7 = ⠛
    patternFromDots(1, 2, 5),     // 8 = ⠓
    patternFromDots(2, 4),        // 9 = ⠊
    patternFromDots(7, 8),        // 10 = ⣀ (tens marker)
    patternFromDots(1, 7, 8),     // 11 = ⣁ (1 + tens marker)
    patternFromDots(1, 2, 7, 8),  // 12 = ⣃ (2 + tens marker)
];

// Modes
const modes = {
    '1': {
        name: 'Default',
        pratyaya: patternFromDots(1),  // ⠁ dot 1
        kriyas: {},
        pratyayaLabels: {}
    },
    '12': {
        name: 'Mode 12',
        pratyaya: patternFromDots(1, 2),  // ⠃ dots 1,2
        kriyas: {},
        pratyayaLabels: {}
    }
};

// Global kriyas - keys are chord sequences joined by '_'
// e.g., '14' = single chord, '14_12' = chord 14 then chord 12
const globalKriyas = {
    '14': { action: 'queryMode' },
    '': { action: 'replay' },  // empty = just space-space with no chords
    '1234': { action: 'clear' },  // all four keys = clear/cancel input
    '13': { action: 'getTime' },  // keys 1+3 = get current time
};

// State
let currentMode = '1';
let chordBuffer = [];
let currentChord = new Set();
let pressedKeys = new Set();
let cancelTimer = null;
let chordTimer = null;  // Timer for chord without space
let lastPratyayaSequence = [];

// Reading queue - for paginated pratyaya output
let readingQueue = [];
let isReading = false;

// DOM
const brailleDisplay = document.getElementById('brailleDisplay');
const bufferDisplay = document.getElementById('bufferDisplay');
const chordOutput = document.getElementById('chordOutput');
const messageEl = document.getElementById('message');
const keyElements = document.querySelectorAll('.key');
const tabsContainer = document.getElementById('modeTabs');
const modePanel = document.getElementById('modePanel');

// Create "more" indicator element
const moreIndicator = document.createElement('div');
moreIndicator.id = 'moreIndicator';
moreIndicator.textContent = '▼';
moreIndicator.style.cssText = 'display:none; text-align:center; font-size:1.5rem; color:#f59e0b; animation:pulse 1s infinite;';
const style = document.createElement('style');
style.textContent = '@keyframes pulse { 0%,100% { opacity:1; } 50% { opacity:0.4; } }';
document.head.appendChild(style);
brailleDisplay.parentNode.insertBefore(moreIndicator, brailleDisplay.nextSibling);

// Display
function showBraille(pattern) {
    brailleDisplay.textContent = patternToBraille(pattern);
}

function showBrailleSequence(patterns, interval = 300, record = true) {
    if (record && patterns.length > 0) {
        lastPratyayaSequence = patterns.filter(p => p.some(v => v));
    }
    let i = 0;
    (function next() {
        if (i < patterns.length) {
            showBraille(patterns[i++]);
            setTimeout(next, interval);
        }
    })();
}

// Paginated reading - space to advance
function startReading(patterns, record = true) {
    if (record && patterns.length > 0) {
        lastPratyayaSequence = patterns.filter(p => p.some(v => v));
    }
    readingQueue = [...patterns];
    isReading = true;
    advanceReading();
}

function advanceReading() {
    if (readingQueue.length === 0) {
        // Done reading
        isReading = false;
        moreIndicator.style.display = 'none';
        showBraille(PRATYAYA.empty);
        showMessage('Ready');
        return;
    }

    // Brief clear before showing next pattern
    showBraille(PRATYAYA.empty);
    setTimeout(() => {
        const pattern = readingQueue.shift();
        showBraille(pattern);

        if (readingQueue.length > 0) {
            moreIndicator.style.display = 'block';
            showMessage(`${readingQueue.length} more - press space`);
        } else {
            moreIndicator.style.display = 'none';
            showMessage('Done - press space');
        }
    }, 80);
}

function cancelReading() {
    readingQueue = [];
    isReading = false;
    moreIndicator.style.display = 'none';
}

function updateBufferDisplay() {
    bufferDisplay.textContent = chordBuffer.join(' ');
}

function updateKeyDisplay() {
    keyElements.forEach(el => {
        el.classList.toggle('pressed', pressedKeys.has(el.dataset.key));
    });
}

function showMessage(msg) {
    messageEl.textContent = msg;
}

function renderModeTabs() {
    tabsContainer.innerHTML = '';
    Object.entries(modes).forEach(([code, mode]) => {
        const tab = document.createElement('div');
        tab.className = 'mode-tab' + (code === currentMode ? ' active' : '');
        const braille = patternToBraille(mode.pratyaya);
        // Always show braille, use different style if empty
        const brailleClass = mode.pratyaya.some(v => v) ? 'tab-braille' : 'tab-braille empty-braille';
        tab.innerHTML = `<b>${code}</b> ${mode.name} <span class="${brailleClass}">${braille || '⠀'}</span>`;
        tab.onclick = () => switchMode(code);
        tabsContainer.appendChild(tab);
    });
}

function renderModePanel() {
    const mode = modes[currentMode];
    const kriyas = Object.entries(mode.kriyas);
    const modePratyayas = Object.entries(mode.pratyayaLabels);
    const globalPratyayasList = Object.entries(globalPratyayas);

    modePanel.innerHTML = `
        <h3>Mode ${currentMode}: ${mode.name}</h3>

        <div class="section">
            <b>Kriyas:</b>
            ${kriyas.length === 0 ? ' <span class="dim">(none)</span>' : ''}
            ${kriyas.map(([c, k]) => `<span class="item"><span class="code">${c}__</span> ${k.label || k.action}</span>`).join('')}
            <button class="add-btn" onclick="promptAddKriya()">+</button>
        </div>

        <div class="section">
            <b>Mode Pratyayas:</b>
            ${modePratyayas.length === 0 ? ' <span class="dim">(none)</span>' : ''}
            ${modePratyayas.map(([p, l]) => {
                const pattern = patternFromDots(...p.split('').map(Number));
                return `<span class="item"><span class="braille">${patternToBraille(pattern)}</span> ${p} = ${l}</span>`;
            }).join('')}
            <button class="add-btn" onclick="promptAddPratyaya()">+</button>
        </div>

        <div class="section">
            <b>Global Pratyayas:</b>
            ${globalPratyayasList.map(([p, obj]) =>
                `<span class="item"><span class="braille">${patternToBraille(obj.pattern)}</span> ${p} = ${obj.label}</span>`
            ).join('')}
        </div>

        <div class="help">
            <b>Global:</b>
            <code>__</code> replay |
            <code>13__</code> time |
            <code>14__</code> query mode |
            <code>14 [m]__</code> switch |
            <code>1234__</code> clear |
            Timeout: 2s
        </div>
        <div class="help" style="font-size: 1rem; margin-top: 0.5rem;">
            <b>Digits:</b>
            <span class="braille" style="font-size: 1.8rem;">⠚</span>=0
            <span class="braille" style="font-size: 1.8rem;">⠁</span>=1
            <span class="braille" style="font-size: 1.8rem;">⠃</span>=2
            <span class="braille" style="font-size: 1.8rem;">⠉</span>=3
            <span class="braille" style="font-size: 1.8rem;">⠙</span>=4
            <span class="braille" style="font-size: 1.8rem;">⠑</span>=5
            <span class="braille" style="font-size: 1.8rem;">⠋</span>=6
            <span class="braille" style="font-size: 1.8rem;">⠛</span>=7
            <span class="braille" style="font-size: 1.8rem;">⠓</span>=8
            <span class="braille" style="font-size: 1.8rem;">⠊</span>=9
            <span class="braille" style="font-size: 1.8rem;">⣀</span>=10
            <span class="braille" style="font-size: 1.8rem;">⣁</span>=11
            <span class="braille" style="font-size: 1.8rem;">⣃</span>=12
        </div>
    `;
}

// Input
function clearTimers() {
    if (cancelTimer) { clearTimeout(cancelTimer); cancelTimer = null; }
    if (chordTimer) { clearTimeout(chordTimer); chordTimer = null; }
}

function startCancelTimer() {
    clearTimers();
    cancelTimer = setTimeout(() => {
        showBrailleSequence([PRATYAYA.cancelled, PRATYAYA.cancelled, PRATYAYA.empty], 200, false);
        showMessage('Cancelled');
        resetInput();
    }, AUTO_CANCEL_MS);
}

function startChordTimer() {
    if (chordTimer) clearTimeout(chordTimer);
    chordTimer = setTimeout(() => {
        showBrailleSequence([PRATYAYA.cancelled, PRATYAYA.cancelled, PRATYAYA.empty], 200, false);
        showMessage('Cancelled');
        resetInput();
    }, AUTO_CANCEL_MS);
}

function getCurrentChordString() {
    return [...currentChord].sort().join('');
}

function resetInput() {
    clearTimers();
    chordBuffer = [];
    currentChord.clear();
    updateBufferDisplay();
    chordOutput.textContent = '';
    showMessage('Ready');
}

function processSpace() {
    const chord = getCurrentChordString();
    clearTimers();

    // If in reading mode and no chord pressed, advance reading
    if (isReading && chord === '') {
        advanceReading();
        return;
    }

    // Any chord cancels reading mode
    if (isReading && chord !== '') {
        cancelReading();
    }

    if (chord !== '') {
        // Chord entered - add to buffer
        chordBuffer.push(chord);
        currentChord.clear();
        updateBufferDisplay();
        chordOutput.textContent = chord;
        startCancelTimer();
    } else {
        // No chord - execute immediately
        executeKriya();
    }
}

// Execution
function executeKriya() {
    clearTimers();

    // Join buffer with '_' to form the kriya key
    const seqStr = chordBuffer.join('_');
    console.log('Execute:', seqStr || '(empty)');

    // Mode switch: 14_<mode>
    if (seqStr.startsWith('14_') && seqStr.length > 3) {
        const targetMode = seqStr.slice(3);
        if (modes[targetMode]) {
            switchMode(targetMode);
        } else {
            showMessage(`Unknown mode: ${targetMode}`);
            showBrailleSequence([PRATYAYA.all, PRATYAYA.empty, PRATYAYA.error, PRATYAYA.error, PRATYAYA.empty], 200);
        }
        resetInput();
        return;
    }

    // Global kriyas
    if (globalKriyas.hasOwnProperty(seqStr)) {
        handleKriya(globalKriyas[seqStr]);
        resetInput();
        return;
    }

    // Mode kriyas
    if (modes[currentMode].kriyas[seqStr]) {
        handleKriya(modes[currentMode].kriyas[seqStr]);
        resetInput();
        return;
    }

    // Unknown
    if (seqStr === '') {
        showMessage('Nothing to do');
    } else {
        showMessage(`Unknown: ${seqStr}`);
        showBrailleSequence([PRATYAYA.error, PRATYAYA.error, PRATYAYA.empty], 200);
    }
    resetInput();
}

function handleKriya(kriya) {
    switch (kriya.action) {
        case 'queryMode':
            showMessage(`Mode: ${currentMode} (${modes[currentMode].name})`);
            const mp = modes[currentMode].pratyaya;
            if (mp.some(v => v)) {
                showBrailleSequence([mp, PRATYAYA.empty], 400);
            } else {
                showBrailleSequence([PRATYAYA.confirm, PRATYAYA.empty], 300);
            }
            break;
        case 'replay':
            if (lastPratyayaSequence.length > 0) {
                showMessage('Replay');
                showBrailleSequence([...lastPratyayaSequence, PRATYAYA.empty], 300, false);
            } else {
                showMessage('Nothing to replay');
            }
            break;
        case 'clear':
            showMessage('Cleared');
            showBraille(PRATYAYA.empty);
            break;
        case 'getTime':
            const now = new Date();
            // Convert to 12-hour format
            let hour12 = now.getHours() % 12;
            if (hour12 === 0) hour12 = 12;
            const mins = now.getMinutes();
            // If minutes <= 12, show as single pattern; otherwise tens + units
            const timePatterns = [DIGIT_PRATYAYA[hour12]];
            if (mins <= 12) {
                timePatterns.push(DIGIT_PRATYAYA[mins]);
            } else {
                timePatterns.push(DIGIT_PRATYAYA[Math.floor(mins / 10)]);
                timePatterns.push(DIGIT_PRATYAYA[mins % 10]);
            }
            startReading(timePatterns);
            break;
        default:
            showMessage(`Action: ${kriya.action}`);
    }
}

function switchMode(code) {
    currentMode = code;
    showMessage(`Mode: ${code}`);
    const mp = modes[code].pratyaya;
    if (mp.some(v => v)) {
        showBrailleSequence([mp, PRATYAYA.empty], 400);
    }
    renderModeTabs();
    renderModePanel();
}

// Add kriya/pratyaya
window.promptAddKriya = function() {
    const code = prompt('Chord sequence (e.g., "1", "12", "14_1"):');
    if (!code || !/^[1-4_]+$/.test(code)) return;
    if (globalKriyas.hasOwnProperty(code) || modes[currentMode].kriyas[code]) {
        alert('Already exists!');
        return;
    }
    const label = prompt('Action label:');
    if (!label) return;
    modes[currentMode].kriyas[code] = { action: 'custom', label };
    renderModePanel();
};

window.promptAddPratyaya = function() {
    const pattern = prompt('Dots (1-8, e.g., "135"):');
    if (!pattern || !/^[1-8]+$/.test(pattern)) return;
    const label = prompt('Label:');
    if (!label) return;
    modes[currentMode].pratyayaLabels[pattern] = label;
    renderModePanel();
};

// Events
document.addEventListener('keydown', (e) => {
    const key = e.key.toLowerCase();
    if (KEY_TO_NUM[key] || key === ' ') {
        e.preventDefault();
        if (!pressedKeys.has(key)) {
            pressedKeys.add(key);
            updateKeyDisplay();
            if (KEY_TO_NUM[key]) {
                currentChord.add(KEY_TO_NUM[key]);
                chordOutput.textContent = getCurrentChordString();
                startChordTimer();  // Start timer when chord keys pressed
            }
        }
    }
});

document.addEventListener('keyup', (e) => {
    const key = e.key.toLowerCase();
    if (pressedKeys.has(key)) {
        pressedKeys.delete(key);
        updateKeyDisplay();
        if (key === ' ') {
            processSpace();
        }
    }
});

window.addEventListener('keydown', (e) => {
    if (e.key === ' ') e.preventDefault();
});

// Init
renderModeTabs();
renderModePanel();
showMessage('Ready');
