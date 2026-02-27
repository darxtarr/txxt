// ============================================================================
// txxt — Browser client
//
// Two sections:
//   1. Overlay class      (PRODUCTION — unchanged from testbed)
//   2. SidecarClient class (PRODUCTION — replaces TestHarness)
//
// Open via backend: cargo run (serves frontend/ at localhost:3000)
// ============================================================================

'use strict';

// ============================================================================
// §1  OVERLAY MODULE — PRODUCTION CODE
//
// A dumb SVG executor. Receives draw commands. Draws them. Knows nothing
// about shapes, SDF, cursor position, or the world.
//
// Command format (mirrors sidecar 0x1A OverlayCommands):
//   { type: 'segment', x1, y1, x2, y2 }
//   { type: 'arc', cx, cy, r, startAngle, sweepAngle }
//   { type: 'clear' }
// ============================================================================

class Overlay {
    /**
     * @param {HTMLElement} container — positioned element to overlay
     * @param {Object} opts
     * @param {number} [opts.segmentPoolSize=48]
     * @param {number} [opts.arcPoolSize=16]
     * @param {string} [opts.color='#00ff66']
     * @param {number} [opts.strokeWidth=2]
     */
    constructor(container, opts = {}) {
        const o = Object.assign({
            segmentPoolSize: 48,
            arcPoolSize: 16,
            color: '#00ff66',
            strokeWidth: 2,
        }, opts);

        this.segmentPoolSize = o.segmentPoolSize;
        this.arcPoolSize = o.arcPoolSize;
        this.color = o.color;
        this.strokeWidth = o.strokeWidth;

        // Create SVG root
        const ns = 'http://www.w3.org/2000/svg';
        this.svg = document.createElementNS(ns, 'svg');
        this.svg.setAttribute('class', 'overlay-svg');
        container.appendChild(this.svg);

        // Allocate segment pool (<line> elements)
        this.segments = [];
        for (let i = 0; i < this.segmentPoolSize; i++) {
            const el = document.createElementNS(ns, 'line');
            el.setAttribute('stroke', this.color);
            el.setAttribute('stroke-width', this.strokeWidth);
            el.setAttribute('stroke-linecap', 'round');
            el.style.opacity = '0';
            this.svg.appendChild(el);
            this.segments.push(el);
        }

        // Allocate arc pool (<path> elements)
        this.arcs = [];
        for (let i = 0; i < this.arcPoolSize; i++) {
            const el = document.createElementNS(ns, 'path');
            el.setAttribute('stroke', this.color);
            el.setAttribute('stroke-width', this.strokeWidth);
            el.setAttribute('stroke-linecap', 'round');
            el.setAttribute('fill', 'none');
            el.style.opacity = '0';
            this.svg.appendChild(el);
            this.arcs.push(el);
        }

        this.activeSegments = 0;
        this.activeArcs = 0;
    }

    /**
     * Map draw commands to pool slots. Active slots get attributes + opacity 1.
     * Inactive slots get opacity 0. CSS transition handles fade.
     *
     * @param {Array} commands — array of draw command objects
     */
    update(commands) {
        let si = 0; // segment index
        let ai = 0; // arc index

        for (let i = 0; i < commands.length; i++) {
            const cmd = commands[i];

            if (cmd.type === 'clear') {
                // Hide everything
                for (let j = 0; j < this.segmentPoolSize; j++) {
                    this.segments[j].style.opacity = '0';
                }
                for (let j = 0; j < this.arcPoolSize; j++) {
                    this.arcs[j].style.opacity = '0';
                }
                si = 0;
                ai = 0;
                continue;
            }

            if (cmd.type === 'segment') {
                if (si < this.segmentPoolSize) {
                    const el = this.segments[si];
                    el.setAttribute('x1', cmd.x1);
                    el.setAttribute('y1', cmd.y1);
                    el.setAttribute('x2', cmd.x2);
                    el.setAttribute('y2', cmd.y2);
                    el.style.opacity = '1';
                    si++;
                }
                // Pool exhausted: silently skip
                continue;
            }

            if (cmd.type === 'arc') {
                if (ai < this.arcPoolSize) {
                    const el = this.arcs[ai];
                    el.setAttribute('d', arcToPath(
                        cmd.cx, cmd.cy, cmd.r, cmd.startAngle, cmd.sweepAngle
                    ));
                    el.style.opacity = '1';
                    ai++;
                }
                continue;
            }
        }

        // Hide unused pool slots
        for (let j = si; j < this.segmentPoolSize; j++) {
            this.segments[j].style.opacity = '0';
        }
        for (let j = ai; j < this.arcPoolSize; j++) {
            this.arcs[j].style.opacity = '0';
        }

        this.activeSegments = si;
        this.activeArcs = ai;
    }

    destroy() {
        if (this.svg && this.svg.parentNode) {
            this.svg.parentNode.removeChild(this.svg);
        }
        this.segments = [];
        this.arcs = [];
    }
}

/**
 * Convert arc params to SVG path d-attribute.
 * startAngle and sweepAngle in radians.
 *
 * SVG arcs cannot represent a full circle (start == end draws nothing),
 * so near-full arcs are split into two halves.
 */
function arcToPath(cx, cy, r, startAngle, sweepAngle) {
    const absSweep = Math.abs(sweepAngle);

    // Full or near-full circle: split into two semicircles
    if (absSweep > Math.PI * 1.999) {
        const half = sweepAngle / 2;
        const midAngle = startAngle + half;
        const x1 = cx + r * Math.cos(startAngle);
        const y1 = cy + r * Math.sin(startAngle);
        const xm = cx + r * Math.cos(midAngle);
        const ym = cy + r * Math.sin(midAngle);
        const x2 = cx + r * Math.cos(startAngle + sweepAngle);
        const y2 = cy + r * Math.sin(startAngle + sweepAngle);
        const sf = sweepAngle > 0 ? 1 : 0;
        return `M ${x1} ${y1} A ${r} ${r} 0 1 ${sf} ${xm} ${ym} A ${r} ${r} 0 1 ${sf} ${x2} ${y2}`;
    }

    const x1 = cx + r * Math.cos(startAngle);
    const y1 = cy + r * Math.sin(startAngle);
    const endAngle = startAngle + sweepAngle;
    const x2 = cx + r * Math.cos(endAngle);
    const y2 = cy + r * Math.sin(endAngle);
    const largeArc = absSweep > Math.PI ? 1 : 0;
    const sweepFlag = sweepAngle > 0 ? 1 : 0;
    return `M ${x1} ${y1} A ${r} ${r} 0 ${largeArc} ${sweepFlag} ${x2} ${y2}`;
}


// ============================================================================
// §2  SIDECAR CLIENT — PRODUCTION CODE
//
// WebSocket connection to the Rust sidecar.
// Decodes binary frames and feeds the Overlay module.
// Sends cursor position at 20Hz and viewport size on connect.
//
// Wire protocol (all little-endian):
//   Sidecar → Browser:
//     0x10 BackgroundImage: [type:u8][rev:u64][format:u8][w:u16][h:u16][...bytes]
//     0x1A OverlayCommands: [type:u8][count:u8] + per-cmd (see below)
//     0x11 CandidateList:   [type:u8][count:u8] + per-candidate (stub)
//
//   Browser → Sidecar:
//     0x20 ViewportSize: [type:u8][w:u16][h:u16]
//     0x23 CursorMove:   [type:u8][x:u16][y:u16]
//
//   Overlay command encoding:
//     Segment: [cmd:u8=2][x1:u16][y1:u16][x2:u16][y2:u16][color:u32]          13 bytes
//     Arc:     [cmd:u8=1][cx:f32][cy:f32][r:f32][start:f32][sweep:f32][color:u32] 25 bytes
//     Clear:   [cmd:u8=0][0×12 padding]                                         13 bytes
// ============================================================================

class SidecarClient {
    /**
     * @param {string} containerId — id of the engine container element
     * @param {string} [wsUrl]     — WebSocket URL (default: ws://localhost:3000/ws)
     */
    constructor(containerId, wsUrl) {
        this.container = document.getElementById(containerId);
        this.wsUrl = wsUrl || 'ws://localhost:3000/ws';
        this.ws = null;
        this._bgUrl = null;

        this.cursorX = 0;
        this.cursorY = 0;
        this._cursorDirty = false;
        this.flashlightRadius = 120; // visual only — sidecar owns the actual radius

        this.showCircle = true;

        // Overlay (production module, identical to testbed)
        this.overlay = new Overlay(this.container, {
            segmentPoolSize: 256,
            arcPoolSize: 64,
            color: '#00ff66',
            strokeWidth: 2,
        });

        // Flashlight circle indicator (visual feedback — matches sidecar radius)
        const ns = 'http://www.w3.org/2000/svg';
        this.flashSvg = document.createElementNS(ns, 'svg');
        this.flashSvg.setAttribute('class', 'flashlight-svg');
        this.container.appendChild(this.flashSvg);

        this.flashCircle = document.createElementNS(ns, 'circle');
        this.flashCircle.setAttribute('stroke', '#ffcc00');
        this.flashCircle.setAttribute('stroke-width', '1.5');
        this.flashCircle.setAttribute('fill', 'none');
        this.flashCircle.setAttribute('stroke-opacity', '0.6');
        this.flashSvg.appendChild(this.flashCircle);

        // Perf display
        this.perfEl = document.getElementById('perf');
        this._setStatus('Connecting…');

        // Mouse tracking
        this.container.addEventListener('mousemove', (e) => {
            const rect = this.container.getBoundingClientRect();
            this.cursorX = Math.round(e.clientX - rect.left);
            this.cursorY = Math.round(e.clientY - rect.top);

            if (this.showCircle) {
                this.flashCircle.setAttribute('cx', this.cursorX);
                this.flashCircle.setAttribute('cy', this.cursorY);
                this.flashCircle.setAttribute('r', this.flashlightRadius);
            }

            this._cursorDirty = true;
        });

        // Click → CREATE_AT (0x27): creates a task at the clicked grid position.
        // The sidecar snaps to 15-min grid, creates the task, re-renders, and
        // broadcasts the new background image. Move cursor over the box to trace it.
        this.container.addEventListener('click', (e) => {
            if (!this.ws || this.ws.readyState !== WebSocket.OPEN) return;
            const rect = this.container.getBoundingClientRect();
            const x = Math.round(e.clientX - rect.left);
            const y = Math.round(e.clientY - rect.top);
            const buf = new ArrayBuffer(5);
            const dv = new DataView(buf);
            dv.setUint8(0, 0x27); // CREATE_AT
            dv.setUint16(1, x, true);
            dv.setUint16(3, y, true);
            this.ws.send(buf);
        });

        // 20Hz cursor send — only when dirty
        this._cursorTimer = setInterval(() => this._sendCursor(), 50);

        this._connect();
    }

    // ── Connection ────────────────────────────────────────────

    _connect() {
        this.ws = new WebSocket(this.wsUrl);
        this.ws.binaryType = 'arraybuffer';

        this.ws.addEventListener('open', () => {
            this._setStatus('Connected — awaiting frame…');
            this._sendViewportSize();
        });

        this.ws.addEventListener('message', (e) => {
            this._onMessage(e.data);
        });

        this.ws.addEventListener('close', () => {
            this._setStatus('Disconnected — reconnecting in 2s…');
            setTimeout(() => this._connect(), 2000);
        });

        this.ws.addEventListener('error', () => {
            // close event fires after error, reconnect happens there
        });
    }

    // ── Outbound messages ─────────────────────────────────────

    _sendViewportSize() {
        const w = this.container.clientWidth;
        const h = this.container.clientHeight;
        const buf = new ArrayBuffer(5);
        const dv = new DataView(buf);
        dv.setUint8(0, 0x20);
        dv.setUint16(1, w, true);
        dv.setUint16(3, h, true);
        this.ws.send(buf);
    }

    _sendCursor() {
        if (!this._cursorDirty) return;
        if (!this.ws || this.ws.readyState !== WebSocket.OPEN) return;
        this._cursorDirty = false;

        const buf = new ArrayBuffer(5);
        const dv = new DataView(buf);
        dv.setUint8(0, 0x23);
        dv.setUint16(1, this.cursorX, true);
        dv.setUint16(3, this.cursorY, true);
        this.ws.send(buf);
    }

    // ── Inbound messages ──────────────────────────────────────

    _onMessage(data) {
        const dv = new DataView(data);
        if (dv.byteLength < 1) return;

        const type = dv.getUint8(0);

        if (type === 0x10) {
            this._onBackgroundImage(dv, data);
        } else if (type === 0x1A) {
            this._onOverlayCommands(dv);
        }
        // 0x11 CandidateList — stub until materialization module
    }

    _onBackgroundImage(dv, data) {
        // [type:u8][rev:u64][format:u8][w:u16][h:u16][...image bytes]
        //  0        1        9           10     12     14
        if (dv.byteLength < 14) return;

        const rev = dv.getBigUint64(1, true);
        const format = dv.getUint8(9);
        const imageBytes = data.slice(14);

        const mimeType = format === 1 ? 'image/jpeg' : 'image/png';
        const blob = new Blob([imageBytes], { type: mimeType });
        const url = URL.createObjectURL(blob);

        if (this._bgUrl) URL.revokeObjectURL(this._bgUrl);
        this._bgUrl = url;

        this.container.style.backgroundImage = `url(${url})`;

        this._setStatus(`Connected | rev ${rev} | ${(imageBytes.byteLength / 1024).toFixed(1)} KB`);
    }

    _onOverlayCommands(dv) {
        // [type:u8][count:u8] then per command:
        //   Segment: [2][x1:u16][y1:u16][x2:u16][y2:u16][color:u32]             13 bytes
        //   Arc:     [1][cx:f32][cy:f32][r:f32][start:f32][sweep:f32][color:u32] 25 bytes
        //   Clear:   [0][0×12]                                                   13 bytes
        if (dv.byteLength < 2) return;

        const count = dv.getUint8(1);
        const commands = [];
        let offset = 2;

        for (let i = 0; i < count; i++) {
            if (offset >= dv.byteLength) break;
            const cmd = dv.getUint8(offset);

            if (cmd === 0) {             // Clear
                commands.push({ type: 'clear' });
                offset += 13;
            } else if (cmd === 2) {      // Segment
                if (offset + 13 > dv.byteLength) break;
                commands.push({
                    type: 'segment',
                    x1: dv.getUint16(offset + 1, true),
                    y1: dv.getUint16(offset + 3, true),
                    x2: dv.getUint16(offset + 5, true),
                    y2: dv.getUint16(offset + 7, true),
                });
                offset += 13;
            } else if (cmd === 1) {      // Arc
                if (offset + 25 > dv.byteLength) break;
                commands.push({
                    type: 'arc',
                    cx:         dv.getFloat32(offset + 1,  true),
                    cy:         dv.getFloat32(offset + 5,  true),
                    r:          dv.getFloat32(offset + 9,  true),
                    startAngle: dv.getFloat32(offset + 13, true),
                    sweepAngle: dv.getFloat32(offset + 17, true),
                });
                offset += 25;
            } else {
                break; // unknown command type — stop parsing
            }
        }

        this.overlay.update(commands);
    }

    // ── Controls (wired from index.html) ──────────────────────

    setFlashlightRadius(r) {
        this.flashlightRadius = r;
        // Update the visual circle immediately (sidecar radius stays at 120)
        this.flashCircle.setAttribute('r', r);
    }

    setShowCircle(show) {
        this.showCircle = show;
        this.flashSvg.style.display = show ? '' : 'none';
    }

    // ── Cleanup ───────────────────────────────────────────────

    destroy() {
        clearInterval(this._cursorTimer);
        if (this.ws) this.ws.close();
        this.overlay.destroy();
        if (this._bgUrl) URL.revokeObjectURL(this._bgUrl);
        if (this.flashSvg.parentNode) this.flashSvg.parentNode.removeChild(this.flashSvg);
    }

    // ── Internal ──────────────────────────────────────────────

    _setStatus(msg) {
        if (this.perfEl) this.perfEl.textContent = msg;
    }
}
