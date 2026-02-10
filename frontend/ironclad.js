/**
 * IRONCLAD ENGINE v0.6 — Server-Connected Build
 *
 * Canvas/DOM hybrid renderer for time tracking on software-rendered CloudPCs.
 * No dependencies. No build step.
 *
 * v0.6: WebSocket connection to txxt game server. Binary wire protocol.
 *        Fixed-stride packed records read via DataView. No JSON.
 *
 * Previous fixes (v0.5):
 *  - Frame-stamp dedup for multi-bucket entities
 *  - Pre-allocated candidate arrays (zero hot-path allocation)
 *  - Flat array spatial index (no Map in hot loop)
 *  - Arrow rAF callback (no .bind() per frame)
 *  - Per-frame drag latency (not cumulative)
 *  - textContent stats (no innerHTML churn)
 *  - Grid snap derived from HOUR_HEIGHT, not magic numbers
 */

// ─── Config ─────────────────────────────────────────────────────────────────

const CONFIG = {
    // Layout (px)
    LEFT_GUTTER: 56,
    TOP_HEADER: 32,
    DAY_WIDTH: 180,
    HOUR_HEIGHT: 60,

    // Time
    DAYS: 7,
    START_HOUR: 8,
    END_HOUR: 18,

    // Engine
    FLASHLIGHT_RADIUS: 150,
    POOL_SIZE: 15,
    BUCKET_WIDTH: 200,
    MAX_ENTITIES: 10000,
    MAX_BUCKETS: 100,
};

const DAY_LABELS = ['MON', 'TUE', 'WED', 'THU', 'FRI', 'SAT', 'SUN'];
const HOURS = CONFIG.END_HOUR - CONFIG.START_HOUR;
const SNAP_Y = CONFIG.HOUR_HEIGHT / 4; // 15-minute grid

// Task label fragments — verbose on purpose, text is dead weight we measure
const LABEL_VERBS = ['Review', 'Update', 'Fix', 'Deploy', 'Test', 'Write', 'Plan', 'Design', 'Debug', 'Refactor'];
const LABEL_NOUNS = ['API docs', 'homepage', 'auth flow', 'dashboard', 'CI pipeline', 'database', 'UI tests', 'sprint plan', 'onboarding', 'backlog'];
const LABEL_REASONS = [
    'Needs sign-off from stakeholders before EOD Friday',
    'Blocked until the upstream API migration completes',
    'Critical path item for the Q3 release milestone',
    'Compliance requirement from the latest security audit',
    'Technical debt accumulated over the last three sprints',
    'Dependency on the shared component library upgrade',
    'Performance regression flagged in last week\'s report',
    'Requested by product owner during sprint planning',
];
const LABEL_BULLETS = [
    'Verify all edge cases against the test matrix',
    'Update the integration tests for new endpoints',
    'Cross-reference with the ServiceNow ticket backlog',
    'Coordinate with DevOps on the deployment window',
    'Document any breaking changes in the changelog',
    'Run load tests against staging before merge',
    'Get peer review from at least two team members',
    'Sync with the design system token updates',
    'Check backwards compatibility with legacy clients',
    'Validate against the accessibility requirements',
];

// Entity type colors: [fill, stroke]
const TYPE_COLORS = [
    ['#1e3a5f', '#3a7bd5'], // Task — blue (Low/Medium priority)
    ['#1a4a3a', '#2d9b7a'], // Event — teal (High priority)
    ['#4a3a1a', '#b4882d'], // Milestone — amber (Urgent priority)
];

// ─── Wire protocol (mirrors backend/src/wire.rs) ────────────────────────────

const WIRE = {
    // Server → Client message types
    SNAPSHOT:         0x01,
    TASK_CREATED:     0x02,
    TASK_SCHEDULED:   0x03,
    TASK_MOVED:       0x04,
    TASK_UNSCHEDULED: 0x05,
    TASK_COMPLETED:   0x06,
    TASK_DELETED:     0x07,

    // Client → Server command types
    CMD_MOVE_TASK:    0x12,

    // Record strides (bytes)
    TASK_STRIDE:      192,
    SERVICE_STRIDE:   80,
    SNAPSHOT_HEADER:  17,

    // Task record field offsets
    TASK_ID:          0,   // 16 bytes UUID
    TASK_STATUS:      16,  // u8
    TASK_PRIORITY:    17,  // u8
    TASK_DAY:         18,  // u8 (0xFF = staged)
    TASK_START_TIME:  20,  // u16 LE
    TASK_DURATION:    22,  // u16 LE
    TASK_SERVICE_ID:  24,  // 16 bytes UUID
    TASK_ASSIGNED_TO: 40,  // 16 bytes UUID
    TASK_TITLE:       56,  // 128 bytes UTF-8 zero-padded

    // Service record field offsets
    SVC_ID:           0,   // 16 bytes UUID
    SVC_NAME:         16,  // 64 bytes UTF-8 zero-padded
};

// ─── Engine ─────────────────────────────────────────────────────────────────

class IroncladEngine {
    constructor(containerId) {
        const el = document.getElementById(containerId);
        if (!el) throw new Error(`#${containerId} not found`);
        this.container = el;
        this.container.style.position = 'relative';
        this.container.style.overflow = 'hidden';

        // ── SoA entity storage ──
        this.count = 0;
        this.ids   = new Int32Array(CONFIG.MAX_ENTITIES);
        this.xs    = new Float32Array(CONFIG.MAX_ENTITIES);
        this.ys    = new Float32Array(CONFIG.MAX_ENTITIES);
        this.ws    = new Float32Array(CONFIG.MAX_ENTITIES);
        this.hs    = new Float32Array(CONFIG.MAX_ENTITIES);
        this.types = new Uint8Array(CONFIG.MAX_ENTITIES);
        this.labels = new Array(CONFIG.MAX_ENTITIES); // strings can't live in typed arrays
        this.uuids = new Uint8Array(CONFIG.MAX_ENTITIES * 16); // 16-byte UUIDs, flat

        // ── Server connection state ──
        this.ws_conn = null;
        this.revision = 0;
        this.connected = false;

        // ── Spatial index: array-of-arrays, reused via length reset ──
        this.buckets = new Array(CONFIG.MAX_BUCKETS);
        for (let i = 0; i < CONFIG.MAX_BUCKETS; i++) this.buckets[i] = [];

        // ── Frame-stamp dedup (entities spanning multiple buckets) ──
        this.frameStamp = new Uint32Array(CONFIG.MAX_ENTITIES);
        this.currentFrame = 0;

        // ── Pre-allocated candidate buffers (zero alloc in hot path) ──
        this.candidateIdx  = new Int32Array(CONFIG.MAX_ENTITIES);
        this.candidateDist = new Float64Array(CONFIG.MAX_ENTITIES);
        this.candidateCount = 0;

        // ── DOM pool ──
        this.pool = [];

        // ── Canvas ──
        this.dpr = window.devicePixelRatio || 1;
        this.canvas = document.createElement('canvas');
        this.canvas.style.cssText = 'position:absolute;top:0;left:0;pointer-events:none;';
        this.container.appendChild(this.canvas);
        this.ctx = this.canvas.getContext('2d', { alpha: false });
        if (!this.ctx) throw new Error('Canvas 2D unavailable');

        // ── Input state ──
        this.mouseX = -9999;
        this.mouseY = -9999;
        this.prevMX = -9999;
        this.prevMY = -9999;
        this.dirty = true;

        // ── Drag state ──
        this.dragIdx = -1;
        this.dragOffX = 0;
        this.dragOffY = 0;
        this.dragInputTime = 0;

        // ── Perf ring buffer ──
        this.frameTimes = new Float64Array(60);
        this.ftHead = 0;
        this.ftCount = 0;
        this.prevTime = 0;
        this.stats = { fps: 0, frameTime: 0, candidates: 0, dragLatency: 0 };

        // ── Stats panel ──
        this._buildStatsPanel();

        // ── Init ──
        this._initPool();
        this._bindInput();
        this._resize();

        // Arrow function — bound once, reused every frame
        this._raf = (now) => {
            this._tick(now);
            requestAnimationFrame(this._raf);
        };

        if (typeof ResizeObserver !== 'undefined') {
            new ResizeObserver(() => this._resize()).observe(this.container);
        }
        window.addEventListener('resize', () => this._resize());
    }

    // ── Public ──────────────────────────────────────────────────────────

    start(n = 500) {
        this._generate(n);
        this.prevTime = performance.now();
        requestAnimationFrame(this._raf);
    }

    setEntityCount(n) {
        this._generate(Math.max(1, Math.min(n | 0, CONFIG.MAX_ENTITIES)));
    }

    setFlashlightRadius(r) {
        CONFIG.FLASHLIGHT_RADIUS = Math.max(20, Math.min(r | 0, 600));
    }

    // ── Server connection ────────────────────────────────────────────

    connect(url) {
        this.prevTime = performance.now();
        requestAnimationFrame(this._raf);

        this._wsConnect(url);
    }

    _wsConnect(url) {
        const ws = new WebSocket(url);
        ws.binaryType = 'arraybuffer';

        ws.onopen = () => {
            console.log('[IRONCLAD] connected to', url);
            this.connected = true;
        };

        ws.onmessage = (e) => {
            if (!(e.data instanceof ArrayBuffer)) return;
            this._handleBinary(new DataView(e.data), e.data);
        };

        ws.onclose = () => {
            console.log('[IRONCLAD] disconnected, reconnecting in 2s...');
            this.connected = false;
            this.ws_conn = null;
            setTimeout(() => this._wsConnect(url), 2000);
        };

        ws.onerror = () => {}; // onclose fires after onerror

        this.ws_conn = ws;
    }

    _handleBinary(view, buffer) {
        if (view.byteLength < 1) return;
        const type = view.getUint8(0);

        switch (type) {
            case WIRE.SNAPSHOT:         this._onSnapshot(view, buffer); break;
            case WIRE.TASK_CREATED:     this._onTaskCreated(view, buffer); break;
            case WIRE.TASK_SCHEDULED:   // fall through — same as moved
            case WIRE.TASK_MOVED:       this._onTaskMoved(view); break;
            case WIRE.TASK_UNSCHEDULED: this._onTaskRemoveFromGrid(view); break;
            case WIRE.TASK_COMPLETED:   this._onTaskRemoveFromGrid(view); break;
            case WIRE.TASK_DELETED:     this._onTaskDeleted(view); break;
            default:
                console.warn('[IRONCLAD] unknown message type:', type);
        }
    }

    // ── Snapshot: populate all SoA arrays from server state ──────────

    _onSnapshot(view, buffer) {
        const rev       = Number(view.getBigUint64(1, true));
        const taskCount = view.getUint32(9, true);
        const svcCount  = view.getUint32(13, true);

        this.revision = rev;

        // Parse tasks — only show Scheduled (1) and Active (2) on the grid
        let idx = 0;
        const base = WIRE.SNAPSHOT_HEADER;

        for (let t = 0; t < taskCount; t++) {
            const off = base + t * WIRE.TASK_STRIDE;
            const status = view.getUint8(off + WIRE.TASK_STATUS);
            const day = view.getUint8(off + WIRE.TASK_DAY);

            // Skip staged (day=0xFF) and completed tasks
            if (day === 0xFF || status === 3) continue;

            this._loadTaskRecord(view, buffer, off, idx);
            idx++;
        }

        this.count = idx;
        this._rebuildIndex();
        this.dirty = true;
        this.prevMX = -9999; // force flashlight refresh
        for (let i = 0; i < this.pool.length; i++) this.pool[i].style.display = 'none';

        console.log(`[IRONCLAD] snapshot: rev=${rev}, ${taskCount} tasks (${idx} on grid), ${svcCount} services`);
    }

    // ── Load one task record into SoA slot ──────────────────────────

    _loadTaskRecord(view, buffer, off, idx) {
        const day       = view.getUint8(off + WIRE.TASK_DAY);
        const startTime = view.getUint16(off + WIRE.TASK_START_TIME, true);
        const duration  = view.getUint16(off + WIRE.TASK_DURATION, true);
        const priority  = view.getUint8(off + WIRE.TASK_PRIORITY);

        // Store UUID (16 bytes)
        const uuidOff = idx * 16;
        const src = new Uint8Array(buffer, off + WIRE.TASK_ID, 16);
        this.uuids.set(src, uuidOff);

        // Convert scheduling data to pixel coordinates
        const pad = 10;
        this.ids[idx]  = idx;
        this.xs[idx]   = CONFIG.LEFT_GUTTER + day * CONFIG.DAY_WIDTH + pad;
        this.ys[idx]   = CONFIG.TOP_HEADER + ((startTime / 60) - CONFIG.START_HOUR) * CONFIG.HOUR_HEIGHT;
        this.ws[idx]   = CONFIG.DAY_WIDTH - pad * 2;
        this.hs[idx]   = (duration / 60) * CONFIG.HOUR_HEIGHT;
        this.types[idx] = priority >= 3 ? 2 : priority >= 2 ? 1 : 0;

        // Decode title from zero-padded UTF-8
        const titleBytes = new Uint8Array(buffer, off + WIRE.TASK_TITLE, 128);
        let titleEnd = 0;
        while (titleEnd < 128 && titleBytes[titleEnd] !== 0) titleEnd++;
        const title = new TextDecoder().decode(titleBytes.subarray(0, titleEnd));

        this.labels[idx] = [title, '', '', '', ''];
    }

    // ── Delta: TaskCreated → add to grid if scheduled ───────────────

    _onTaskCreated(view, buffer) {
        const rev = Number(view.getBigUint64(1, true));
        this.revision = rev;

        // Task record starts at offset 9 (after type + revision)
        const off = 9;
        const status = view.getUint8(off + WIRE.TASK_STATUS);
        const day = view.getUint8(off + WIRE.TASK_DAY);

        // Only add to grid if scheduled
        if (day === 0xFF || status === 3) return;

        const idx = this.count;
        this._loadTaskRecord(view, buffer, off, idx);
        this.count++;
        this._rebuildIndex();
        this.dirty = true;
    }

    // ── Delta: TaskScheduled / TaskMoved → update position ──────────

    _onTaskMoved(view) {
        const rev = Number(view.getBigUint64(1, true));
        this.revision = rev;

        const uuidBytes = new Uint8Array(view.buffer, view.byteOffset + 9, 16);
        const idx = this._findByUuid(uuidBytes);

        const day       = view.getUint8(25);
        const startTime = view.getUint16(26, true);
        const duration  = view.getUint16(28, true);

        if (idx === -1) {
            // Task was staged, now scheduled — need to add it
            // Build a minimal task record in the SoA arrays
            const newIdx = this.count;
            const pad = 10;
            this.uuids.set(uuidBytes, newIdx * 16);
            this.ids[newIdx]  = newIdx;
            this.xs[newIdx]   = CONFIG.LEFT_GUTTER + day * CONFIG.DAY_WIDTH + pad;
            this.ys[newIdx]   = CONFIG.TOP_HEADER + ((startTime / 60) - CONFIG.START_HOUR) * CONFIG.HOUR_HEIGHT;
            this.ws[newIdx]   = CONFIG.DAY_WIDTH - pad * 2;
            this.hs[newIdx]   = (duration / 60) * CONFIG.HOUR_HEIGHT;
            this.types[newIdx] = 0;
            this.labels[newIdx] = ['(new task)', '', '', '', ''];
            this.count++;
        } else {
            const pad = 10;
            this.xs[idx] = CONFIG.LEFT_GUTTER + day * CONFIG.DAY_WIDTH + pad;
            this.ys[idx] = CONFIG.TOP_HEADER + ((startTime / 60) - CONFIG.START_HOUR) * CONFIG.HOUR_HEIGHT;
            this.hs[idx] = (duration / 60) * CONFIG.HOUR_HEIGHT;
        }

        this._rebuildIndex();
        this.dirty = true;
    }

    // ── Delta: Unschedule/Complete → remove from grid ───────────────

    _onTaskRemoveFromGrid(view) {
        const rev = Number(view.getBigUint64(1, true));
        this.revision = rev;

        const uuidBytes = new Uint8Array(view.buffer, view.byteOffset + 9, 16);
        const idx = this._findByUuid(uuidBytes);
        if (idx === -1) return;

        this._swapRemove(idx);
        this._rebuildIndex();
        this.dirty = true;
    }

    // ── Delta: TaskDeleted → remove from grid ───────────────────────

    _onTaskDeleted(view) {
        const rev = Number(view.getBigUint64(1, true));
        this.revision = rev;

        const uuidBytes = new Uint8Array(view.buffer, view.byteOffset + 9, 16);
        const idx = this._findByUuid(uuidBytes);
        if (idx === -1) return;

        this._swapRemove(idx);
        this._rebuildIndex();
        this.dirty = true;
    }

    // ── Send MoveTask command to server ─────────────────────────────

    _sendMoveTask(entityIdx) {
        if (!this.ws_conn || this.ws_conn.readyState !== WebSocket.OPEN) return;

        // Convert pixel position back to scheduling data
        const col = Math.round((this.xs[entityIdx] - CONFIG.LEFT_GUTTER - 10) / CONFIG.DAY_WIDTH);
        const day = Math.max(0, Math.min(col, CONFIG.DAYS - 1));

        const relMinutes = ((this.ys[entityIdx] - CONFIG.TOP_HEADER) / CONFIG.HOUR_HEIGHT + CONFIG.START_HOUR) * 60;
        const startTime = Math.round(relMinutes / 15) * 15;

        const relDur = (this.hs[entityIdx] / CONFIG.HOUR_HEIGHT) * 60;
        const duration = Math.max(15, Math.round(relDur / 15) * 15);

        // Pack: [type:u8][task_id:16][day:u8][start_time:u16 LE][duration:u16 LE]
        const buf = new ArrayBuffer(22);
        const v = new DataView(buf);
        const arr = new Uint8Array(buf);

        arr[0] = WIRE.CMD_MOVE_TASK;
        arr.set(this.uuids.subarray(entityIdx * 16, entityIdx * 16 + 16), 1);
        arr[17] = day;
        v.setUint16(18, startTime, true);
        v.setUint16(20, duration, true);

        this.ws_conn.send(buf);
    }

    // ── Helpers ──────────────────────────────────────────────────────

    _findByUuid(uuidBytes) {
        for (let i = 0; i < this.count; i++) {
            const off = i * 16;
            let match = true;
            for (let b = 0; b < 16; b++) {
                if (this.uuids[off + b] !== uuidBytes[b]) { match = false; break; }
            }
            if (match) return i;
        }
        return -1;
    }

    _swapRemove(idx) {
        const last = this.count - 1;
        if (idx !== last) {
            // Copy last entity into the removed slot
            this.ids[idx]   = this.ids[last];
            this.xs[idx]    = this.xs[last];
            this.ys[idx]    = this.ys[last];
            this.ws[idx]    = this.ws[last];
            this.hs[idx]    = this.hs[last];
            this.types[idx] = this.types[last];
            this.labels[idx] = this.labels[last];
            this.uuids.copyWithin(idx * 16, last * 16, last * 16 + 16);
        }
        this.count--;
    }

    // ── Pool ────────────────────────────────────────────────────────────

    _initPool() {
        for (let i = 0; i < CONFIG.POOL_SIZE; i++) {
            const d = document.createElement('div');
            d.className = 'proxy';
            d.style.cssText =
                'position:absolute;display:none;box-sizing:border-box;' +
                'border:2px solid #00ffcc;background:rgba(0,255,204,0.06);' +
                'cursor:grab;z-index:10;will-change:transform;' +
                'border-radius:3px;';
            this.container.appendChild(d);
            this.pool.push(d);
        }
    }

    // ── Spatial index ───────────────────────────────────────────────────

    _rebuildIndex() {
        for (let b = 0; b < CONFIG.MAX_BUCKETS; b++) this.buckets[b].length = 0;

        for (let i = 0; i < this.count; i++) {
            const b0 = (this.xs[i] / CONFIG.BUCKET_WIDTH) | 0;
            const b1 = ((this.xs[i] + this.ws[i]) / CONFIG.BUCKET_WIDTH) | 0;
            for (let b = b0; b <= b1; b++) {
                if (b >= 0 && b < CONFIG.MAX_BUCKETS) this.buckets[b].push(i);
            }
        }
    }

    // ── Tick ────────────────────────────────────────────────────────────

    _tick(now) {
        const dt = now - this.prevTime;
        this.prevTime = now;

        this.frameTimes[this.ftHead] = dt;
        this.ftHead = (this.ftHead + 1) % 60;
        if (this.ftCount < 60) this.ftCount++;

        const moved = this.mouseX !== this.prevMX || this.mouseY !== this.prevMY;
        if (moved || this.dragIdx >= 0) {
            this._flashlight();
            this.prevMX = this.mouseX;
            this.prevMY = this.mouseY;
        }

        if (this.dirty) {
            this._render();
            this.dirty = false;
        }

        if (this.dragIdx >= 0 && this.dragInputTime > 0) {
            this.stats.dragLatency = performance.now() - this.dragInputTime;
        }

        this._showStats();
    }

    // ── Flashlight ──────────────────────────────────────────────────────

    _flashlight() {
        this.candidateCount = 0;
        this.currentFrame++;

        const rSq = CONFIG.FLASHLIGHT_RADIUS * CONFIG.FLASHLIGHT_RADIUS;
        const cb = (this.mouseX / CONFIG.BUCKET_WIDTH) | 0;

        for (let b = cb - 1; b <= cb + 1; b++) {
            if (b < 0 || b >= CONFIG.MAX_BUCKETS) continue;
            const bk = this.buckets[b];
            for (let j = 0, len = bk.length; j < len; j++) {
                const i = bk[j];

                if (this.frameStamp[i] === this.currentFrame) continue;
                this.frameStamp[i] = this.currentFrame;

                // SDF: distance from cursor to nearest point on rect edge
                // 0 when cursor is inside the rect, positive outside
                const dx = Math.max(this.xs[i] - this.mouseX, this.mouseX - this.xs[i] - this.ws[i], 0);
                const dy = Math.max(this.ys[i] - this.mouseY, this.mouseY - this.ys[i] - this.hs[i], 0);
                const dSq = dx * dx + dy * dy;

                if (dSq <= rSq) {
                    const c = this.candidateCount++;
                    this.candidateIdx[c] = i;
                    this.candidateDist[c] = dSq;
                }
            }
        }

        this.stats.candidates = this.candidateCount;

        // Insertion sort — typically <50 candidates
        for (let i = 1; i < this.candidateCount; i++) {
            const kd = this.candidateDist[i];
            const ki = this.candidateIdx[i];
            let j = i - 1;
            while (j >= 0 && this.candidateDist[j] > kd) {
                this.candidateDist[j + 1] = this.candidateDist[j];
                this.candidateIdx[j + 1] = this.candidateIdx[j];
                j--;
            }
            this.candidateDist[j + 1] = kd;
            this.candidateIdx[j + 1] = ki;
        }

        const n = Math.min(this.candidateCount, CONFIG.POOL_SIZE);
        for (let i = 0; i < CONFIG.POOL_SIZE; i++) {
            const p = this.pool[i];
            if (i < n) {
                const idx = this.candidateIdx[i];
                if (p.dataset.idx !== String(idx)) {
                    p.dataset.idx = String(idx);
                    p.style.width = this.ws[idx] + 'px';
                    p.style.height = this.hs[idx] + 'px';
                }
                p.style.transform = `translate(${this.xs[idx]}px,${this.ys[idx]}px)`;
                if (p.style.display !== 'block') p.style.display = 'block';
            } else {
                if (p.style.display !== 'none') p.style.display = 'none';
            }
        }
    }

    // ── Canvas render ───────────────────────────────────────────────────

    _render() {
        const W = this.canvas.width;
        const H = this.canvas.height;
        const ctx = this.ctx;
        const dpr = this.dpr;

        ctx.fillStyle = '#0e0e12';
        ctx.fillRect(0, 0, W, H);
        ctx.save();
        ctx.scale(dpr, dpr);

        const lw = W / dpr;
        const lh = H / dpr;
        const gx = CONFIG.LEFT_GUTTER;
        const gy = CONFIG.TOP_HEADER;
        const dw = CONFIG.DAY_WIDTH;
        const hh = CONFIG.HOUR_HEIGHT;
        const gridR = gx + CONFIG.DAYS * dw;
        const gridB = gy + HOURS * hh;

        // Day headers
        ctx.fillStyle = '#999';
        ctx.font = '600 11px monospace';
        ctx.textAlign = 'center';
        for (let d = 0; d < CONFIG.DAYS; d++) {
            ctx.fillText(DAY_LABELS[d], gx + d * dw + dw * 0.5, gy - 10);
        }

        // Hour labels
        ctx.fillStyle = '#555';
        ctx.font = '10px monospace';
        ctx.textAlign = 'right';
        for (let h = 0; h <= HOURS; h++) {
            const label = String(CONFIG.START_HOUR + h).padStart(2, '0') + ':00';
            ctx.fillText(label, gx - 8, gy + h * hh + 4);
        }

        // Hour grid lines
        ctx.strokeStyle = '#252530';
        ctx.lineWidth = 1;
        ctx.beginPath();
        for (let h = 0; h <= HOURS; h++) {
            const y = gy + h * hh + 0.5;
            ctx.moveTo(gx, y);
            ctx.lineTo(gridR, y);
        }
        ctx.stroke();

        // 15-min sub-lines
        ctx.strokeStyle = '#18181f';
        ctx.beginPath();
        for (let h = 0; h < HOURS; h++) {
            for (let q = 1; q < 4; q++) {
                const y = gy + h * hh + q * SNAP_Y + 0.5;
                ctx.moveTo(gx, y);
                ctx.lineTo(gridR, y);
            }
        }
        ctx.stroke();

        // Day dividers
        ctx.strokeStyle = '#252530';
        ctx.beginPath();
        for (let d = 0; d <= CONFIG.DAYS; d++) {
            const x = gx + d * dw + 0.5;
            ctx.moveTo(x, gy);
            ctx.lineTo(x, gridB);
        }
        ctx.stroke();

        // Entities — rect + text per entity so overlapping z-order is correct
        const LINE_H = 14;
        const TEXT_PAD = 6;
        const TITLE_FONT = '600 11px -apple-system,BlinkMacSystemFont,"Segoe UI",system-ui,sans-serif';
        const BULLET_FONT = '10px -apple-system,BlinkMacSystemFont,"Segoe UI",system-ui,sans-serif';
        ctx.textBaseline = 'top';
        ctx.textAlign = 'left';

        for (let i = 0; i < this.count; i++) {
            const ex = this.xs[i];
            const ey = this.ys[i];
            const ew = this.ws[i];
            const eh = this.hs[i];
            if (ey > lh || ey + eh < 0) continue;

            const t = this.types[i];
            ctx.fillStyle = TYPE_COLORS[t][0];
            ctx.fillRect(ex, ey, ew, eh);
            ctx.strokeStyle = TYPE_COLORS[t][1];
            ctx.strokeRect(ex, ey, ew, eh);

            if (eh < 20) continue;

            // Clip text to entity bounds
            ctx.save();
            ctx.beginPath();
            ctx.rect(ex, ey, ew, eh);
            ctx.clip();

            const lines = this.labels[i];
            let ty = ey + 4;

            ctx.font = TITLE_FONT;
            ctx.fillStyle = '#ddd';
            ctx.fillText(lines[0], ex + TEXT_PAD, ty);
            ty += LINE_H;

            if (ty + LINE_H <= ey + eh) {
                ctx.font = BULLET_FONT;
                ctx.fillStyle = '#888';

                for (let l = 1; l < 5; l++) {
                    if (ty + LINE_H > ey + eh) break;
                    ctx.fillText('- ' + lines[l], ex + TEXT_PAD, ty);
                    ty += LINE_H;
                }
            }

            ctx.restore();
        }

        ctx.restore();
    }

    // ── Input ───────────────────────────────────────────────────────────

    _bindInput() {
        this.container.addEventListener('mousemove', (e) => {
            const r = this.container.getBoundingClientRect();
            this.mouseX = e.clientX - r.left;
            this.mouseY = e.clientY - r.top;

            if (this.dragIdx >= 0) {
                this.dragInputTime = performance.now();
                this.xs[this.dragIdx] = this.mouseX - this.dragOffX;
                this.ys[this.dragIdx] = this.mouseY - this.dragOffY;
                this.dirty = true;
            }
        });

        this.container.addEventListener('mousedown', (e) => {
            const t = e.target;
            if (!t.classList || !t.classList.contains('proxy')) return;
            const idx = parseInt(t.dataset.idx);
            if (isNaN(idx) || idx < 0 || idx >= this.count) return;

            this.dragIdx = idx;
            this.dragOffX = this.mouseX - this.xs[idx];
            this.dragOffY = this.mouseY - this.ys[idx];
            t.style.cursor = 'grabbing';
            e.preventDefault();
        });

        window.addEventListener('mouseup', () => {
            if (this.dragIdx < 0) return;
            const i = this.dragIdx;

            // Snap Y to 15-min grid, relative to header offset
            const relY = this.ys[i] - CONFIG.TOP_HEADER;
            this.ys[i] = Math.round(relY / SNAP_Y) * SNAP_Y + CONFIG.TOP_HEADER;

            // Snap X to day column
            const col = Math.round((this.xs[i] - CONFIG.LEFT_GUTTER - 10) / CONFIG.DAY_WIDTH);
            const clamped = Math.max(0, Math.min(col, CONFIG.DAYS - 1));
            this.xs[i] = CONFIG.LEFT_GUTTER + clamped * CONFIG.DAY_WIDTH + 10;

            // Send move command to server (if connected)
            if (this.connected) {
                this._sendMoveTask(i);
            }

            this._rebuildIndex();
            this.dirty = true;
            this.dragIdx = -1;
            this.dragInputTime = 0;
            for (let p = 0; p < this.pool.length; p++) this.pool[p].style.cursor = 'grab';
        });
    }

    // ── Resize ──────────────────────────────────────────────────────────

    _resize() {
        const r = this.container.getBoundingClientRect();
        this.canvas.width = r.width * this.dpr;
        this.canvas.height = r.height * this.dpr;
        this.canvas.style.width = r.width + 'px';
        this.canvas.style.height = r.height + 'px';
        this.dirty = true;
    }

    // ── Stats ───────────────────────────────────────────────────────────

    _buildStatsPanel() {
        const el = document.createElement('div');
        el.className = 'perf-stats';
        this.container.appendChild(el);

        this._spans = {};
        for (const k of ['fps', 'frame', 'entities', 'candidates', 'drag']) {
            const s = document.createElement('div');
            el.appendChild(s);
            this._spans[k] = s;
        }
    }

    _showStats() {
        let sum = 0;
        for (let i = 0; i < this.ftCount; i++) sum += this.frameTimes[i];
        const avg = this.ftCount > 0 ? sum / this.ftCount : 16.67;

        this.stats.fps = Math.round(1000 / avg);
        this.stats.frameTime = Math.round(avg * 10) / 10;

        const s = this._spans;
        s.fps.textContent        = 'FPS: ' + this.stats.fps;
        s.frame.textContent      = 'Frame: ' + this.stats.frameTime + 'ms';
        s.entities.textContent   = 'Entities: ' + this.count;
        s.candidates.textContent = 'Near cursor: ' + this.stats.candidates;
        s.drag.textContent = this.dragIdx >= 0
            ? 'Drag: ' + this.stats.dragLatency.toFixed(1) + 'ms'
            : 'Drag: idle';
    }

    // ── Data generation ─────────────────────────────────────────────────

    _generate(count) {
        const gx = CONFIG.LEFT_GUTTER;
        const gy = CONFIG.TOP_HEADER;
        const dw = CONFIG.DAY_WIDTH;
        const hh = CONFIG.HOUR_HEIGHT;
        const pad = 10;

        for (let i = 0; i < count; i++) {
            const col = (Math.random() * CONFIG.DAYS) | 0;
            const hour = Math.random() * (HOURS - 1);
            const dur = 0.25 + Math.random() * 2.75; // 15min — 3h

            this.ids[i]   = i;
            this.xs[i]    = gx + col * dw + pad;
            this.ys[i]    = gy + hour * hh;
            this.ws[i]    = dw - pad * 2;
            this.hs[i]    = dur * hh;
            this.types[i] = (Math.random() * 3) | 0;

            const verb = LABEL_VERBS[(Math.random() * LABEL_VERBS.length) | 0];
            const noun = LABEL_NOUNS[(Math.random() * LABEL_NOUNS.length) | 0];
            const reason = LABEL_REASONS[(Math.random() * LABEL_REASONS.length) | 0];
            this.labels[i] = [
                verb + ' ' + noun + ': ' + reason,
                LABEL_BULLETS[(Math.random() * LABEL_BULLETS.length) | 0],
                LABEL_BULLETS[(Math.random() * LABEL_BULLETS.length) | 0],
                LABEL_BULLETS[(Math.random() * LABEL_BULLETS.length) | 0],
                LABEL_BULLETS[(Math.random() * LABEL_BULLETS.length) | 0],
            ];
        }

        this.count = count;
        this._rebuildIndex();
        this.dirty = true;

        // Force flashlight refresh on next frame
        this.prevMX = -9999;
        // Hide stale proxies
        for (let i = 0; i < this.pool.length; i++) this.pool[i].style.display = 'none';
    }
}
