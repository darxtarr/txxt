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
//     0x27 CreateAt:     [type:u8][x:u16][y:u16]
//
//   Overlay command encoding:
//     Segment: [cmd:u8=2][x1:u16][y1:u16][x2:u16][y2:u16][color:u32]             13 bytes
//     Arc:     [cmd:u8=1][cx:f32][cy:f32][r:f32][start:f32][sweep:f32][color:u32] 25 bytes
//     Clear:   [cmd:u8=0][0×12 padding]                                            13 bytes
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
        this._connected = false;

        this.cursorX = 0;
        this.cursorY = 0;
        this._cursorDirty = false;
        this.flashlightRadius = 120; // visual only — sidecar owns the actual radius
        this.showCircle = true;

        // Perf state
        this._fpsSamples   = [];
        this._svgMs        = 0;
        this._latencyMs    = 0;
        this._activeSegs   = 0;
        this._activeArcs   = 0;
        this._msgCount     = 0;   // messages received this second
        this._msgRate      = 0;   // messages/s (updated every second)
        this._lastCursorSendTime  = 0;
        this._lastOverlayLogTime  = 0;

        // Overlay (production module, identical to testbed)
        this.overlay = new Overlay(this.container, {
            segmentPoolSize: 256,
            arcPoolSize: 64,
            color: '#00ff66',
            strokeWidth: 2,
        });

        // Flashlight circle indicator
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

        // UI elements
        this.perfEl = document.getElementById('perf');
        this._logEl = document.getElementById('log');

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

        // Click → CREATE_AT (0x27)
        this.container.addEventListener('click', (e) => {
            if (!this.ws || this.ws.readyState !== WebSocket.OPEN) return;
            const rect = this.container.getBoundingClientRect();
            const x = Math.round(e.clientX - rect.left);
            const y = Math.round(e.clientY - rect.top);
            const buf = new ArrayBuffer(5);
            const dv = new DataView(buf);
            dv.setUint8(0, 0x27);
            dv.setUint16(1, x, true);
            dv.setUint16(3, y, true);
            this.ws.send(buf);
            this._log('click', `CREATE_AT (${x}, ${y})`);
        });

        // 1Hz msg/s counter
        this._rateTimer = setInterval(() => {
            this._msgRate = this._msgCount;
            this._msgCount = 0;
        }, 1000);

        // rAF loop — cursor send + FPS tracking + perf display.
        // Cursor send is deliberately inside rAF: naturally synchronized with
        // the display refresh rate (32Hz on CloudPC). No fixed-Hz timer.
        this._rafHandle = requestAnimationFrame(t => this._rafLoop(t));

        // Viewport resize — ResizeObserver on the container fires on any size
        // change (window resize, panel collapse, etc.), not just window.resize.
        this._resizeObserver = new ResizeObserver(() => {
            if (this.ws && this.ws.readyState === WebSocket.OPEN) {
                this._sendViewportSize();
            }
        });
        this._resizeObserver.observe(this.container);

        // Visibility / focus loss — clear overlay immediately when page is
        // hidden so stale lines don't linger when the user alt-tabs back.
        this._onVisibilityChange = () => {
            if (document.hidden) {
                this.overlay.update([]);
                this.flashCircle.setAttribute('r', '0');
                this._send0x22(false);
            } else {
                // Returning: re-send viewport (may have changed while hidden),
                // dirty cursor so rAF sends a fresh position on next frame.
                this._send0x22(true);
                if (this.ws && this.ws.readyState === WebSocket.OPEN) {
                    this._sendViewportSize();
                }
                this._cursorDirty = true;
            }
        };
        document.addEventListener('visibilitychange', this._onVisibilityChange);

        // blur/focus covers focus loss without a full tab switch
        this._onBlur  = () => { this.overlay.update([]); this.flashCircle.setAttribute('r', '0'); };
        this._onFocus = () => { this._cursorDirty = true; };
        window.addEventListener('blur',  this._onBlur);
        window.addEventListener('focus', this._onFocus);

        this._log('sys', 'starting — connecting to ' + this.wsUrl);
        this._connect();
    }

    // ── Connection ────────────────────────────────────────────

    _connect() {
        this.ws = new WebSocket(this.wsUrl);
        this.ws.binaryType = 'arraybuffer';

        this.ws.addEventListener('open', () => {
            this._connected = true;
            this._log('ws', 'connected');
            this._sendViewportSize();
        });

        this.ws.addEventListener('message', (e) => {
            this._msgCount++;
            this._onMessage(e.data);
        });

        this.ws.addEventListener('close', () => {
            this._connected = false;
            this._log('ws', 'disconnected — reconnecting in 2s');
            if (this.perfEl) this.perfEl.textContent = 'Disconnected — reconnecting…';
            setTimeout(() => this._connect(), 2000);
        });

        this.ws.addEventListener('error', () => {
            // close fires after error, reconnect happens there
        });
    }

    // ── Outbound ──────────────────────────────────────────────

    _sendViewportSize() {
        const w = this.container.clientWidth;
        const h = this.container.clientHeight;
        const buf = new ArrayBuffer(5);
        const dv = new DataView(buf);
        dv.setUint8(0, 0x20);
        dv.setUint16(1, w, true);
        dv.setUint16(3, h, true);
        this.ws.send(buf);
        this._log('ws', `sent ViewportSize ${w}×${h}`);
    }

    _send0x22(visible) {
        if (!this.ws || this.ws.readyState !== WebSocket.OPEN) return;
        const buf = new ArrayBuffer(10);
        const dv = new DataView(buf);
        dv.setUint8(0, 0x22);
        dv.setUint8(1, visible ? 1 : 0);
        dv.setBigUint64(2, 0n, true); // last_seen_rev — 0 for now
        this.ws.send(buf);
    }

    // ── Inbound ───────────────────────────────────────────────

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
        //  0        1        9          10     12     14
        if (dv.byteLength < 14) return;

        const rev = dv.getBigUint64(1, true);
        const format = dv.getUint8(9);
        const w = dv.getUint16(10, true);
        const h = dv.getUint16(12, true);
        const imageBytes = data.slice(14);
        const kb = (imageBytes.byteLength / 1024).toFixed(1);

        const mimeType = format === 1 ? 'image/jpeg' : 'image/png';
        const blob = new Blob([imageBytes], { type: mimeType });
        const url = URL.createObjectURL(blob);

        if (this._bgUrl) URL.revokeObjectURL(this._bgUrl);
        this._bgUrl = url;
        this.container.style.backgroundImage = `url(${url})`;

        this._log('bg', `rev=${rev}  ${w}×${h}  ${kb} KB  (${mimeType.split('/')[1]})`);
    }

    _onOverlayCommands(dv) {
        // [type:u8][count:u8] then per command:
        //   Segment: [2][x1:u16][y1:u16][x2:u16][y2:u16][color:u32]             13 bytes
        //   Arc:     [1][cx:f32][cy:f32][r:f32][start:f32][sweep:f32][color:u32] 25 bytes
        //   Clear:   [0][0×12]                                                   13 bytes
        if (dv.byteLength < 2) return;

        // Latency: time from last cursor send to this overlay response
        const latency = this._lastCursorSendTime > 0
            ? performance.now() - this._lastCursorSendTime
            : 0;
        this._latencyMs = latency;

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
                break;
            }
        }

        const svgStart = performance.now();
        this.overlay.update(commands);
        this._svgMs     = performance.now() - svgStart;
        this._activeSegs = this.overlay.activeSegments;
        this._activeArcs = this.overlay.activeArcs;

        // Log overlay snapshot at most once per second (only when drawing something)
        const now = performance.now();
        if (now - this._lastOverlayLogTime > 1000 && (this._activeSegs + this._activeArcs) > 0) {
            this._lastOverlayLogTime = now;
            this._log('ov',
                `${this._activeSegs}s ${this._activeArcs}a  ` +
                `svg=${this._svgMs.toFixed(2)}ms  lat=~${latency.toFixed(0)}ms`
            );
        }
    }

    // ── rAF loop — cursor send + FPS + perf display ──────────

    _rafLoop(now) {
        this._rafHandle = requestAnimationFrame(t => this._rafLoop(t));

        // Cursor send: once per frame, only when dirty and connected.
        // Synchronized with display refresh (32Hz on CloudPC, 60Hz local).
        if (this._cursorDirty && this.ws && this.ws.readyState === WebSocket.OPEN) {
            this._cursorDirty = false;
            this._lastCursorSendTime = performance.now();
            const buf = new ArrayBuffer(5);
            const dv = new DataView(buf);
            dv.setUint8(0, 0x23);
            dv.setUint16(1, this.cursorX, true);
            dv.setUint16(3, this.cursorY, true);
            this.ws.send(buf);
        }

        this._fpsSamples.push(now);
        while (this._fpsSamples.length > 61) this._fpsSamples.shift();

        // Update display every 15 frames (~4× per second at 60fps)
        if (this._fpsSamples.length % 15 === 0) this._updatePerfDisplay();
    }

    _updatePerfDisplay() {
        if (!this.perfEl || !this._connected) return;

        const fps = this._fpsSamples.length >= 2
            ? (this._fpsSamples.length - 1) /
              ((this._fpsSamples[this._fpsSamples.length - 1] - this._fpsSamples[0]) / 1000)
            : 0;

        this.perfEl.textContent =
            `FPS: ${fps.toFixed(0)}  SVG: ${this._svgMs.toFixed(2)}ms  Lat: ~${this._latencyMs.toFixed(0)}ms\n` +
            `Overlay: ${this._activeSegs}/256 segs  ${this._activeArcs}/64 arcs\n` +
            `WS in: ${this._msgRate} msg/s`;
    }

    // ── Log ───────────────────────────────────────────────────

    _log(tag, msg) {
        const el = this._logEl;
        if (!el) return;

        const now = new Date();
        const ts = now.toTimeString().slice(0, 8) + '.' +
                   String(now.getMilliseconds()).padStart(3, '0');

        const line = document.createElement('div');
        line.textContent = `${ts}  ${tag.padEnd(5)}  ${msg}`;
        el.appendChild(line);

        // Keep last 200 entries
        while (el.children.length > 200) el.removeChild(el.firstChild);

        // Auto-scroll if already near bottom
        const nearBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
        if (nearBottom) el.scrollTop = el.scrollHeight;
    }

    // ── Controls ──────────────────────────────────────────────

    setFlashlightRadius(r) {
        this.flashlightRadius = r;
        this.flashCircle.setAttribute('r', r);
    }

    setShowCircle(show) {
        this.showCircle = show;
        this.flashSvg.style.display = show ? '' : 'none';
    }

    // ── Log download ──────────────────────────────────────────

    saveLog() {
        const el = this._logEl;
        if (!el) return;
        const lines = Array.from(el.children).map(d => d.textContent).join('\n');
        const blob = new Blob([lines], { type: 'text/plain' });
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = `txxt-log-${new Date().toISOString().replace(/[:.]/g, '-')}.txt`;
        a.click();
        URL.revokeObjectURL(url);
    }

    // ── Cleanup ───────────────────────────────────────────────

    destroy() {
        cancelAnimationFrame(this._rafHandle);
        clearInterval(this._rateTimer);
        this._resizeObserver.disconnect();
        document.removeEventListener('visibilitychange', this._onVisibilityChange);
        window.removeEventListener('blur',  this._onBlur);
        window.removeEventListener('focus', this._onFocus);
        if (this.ws) this.ws.close();
        this.overlay.destroy();
        if (this._bgUrl) URL.revokeObjectURL(this._bgUrl);
        if (this.flashSvg.parentNode) this.flashSvg.parentNode.removeChild(this.flashSvg);
    }
}
