// ============================================================================
// IRONCLAD — Flashlight Overlay Stress Test
//
// Two sections:
//   1. Overlay class      (PRODUCTION — survives into final IRONCLAD)
//   2. TestHarness class  (THROWAWAY — replaced by Rust sidecar)
//
// Open frontend/index.html in a browser. No server needed.
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
// §2  TEST HARNESS — THROWAWAY CODE
//
// Simulates what the Rust sidecar does. Everything below this line gets
// deleted when the real sidecar exists. Clearly separated from production.
// ============================================================================

// ── SDF functions ───────────────────────────────────────────────────────────

function sdfRect(px, py, rx, ry, rw, rh) {
    // Signed distance from point to rectangle boundary
    const cx = rx + rw / 2;
    const cy = ry + rh / 2;
    const dx = Math.abs(px - cx) - rw / 2;
    const dy = Math.abs(py - cy) - rh / 2;
    const outside = Math.sqrt(Math.max(dx, 0) ** 2 + Math.max(dy, 0) ** 2);
    const inside = Math.min(Math.max(dx, dy), 0);
    return outside + inside;
}

function sdfCircle(px, py, cx, cy, r) {
    return Math.sqrt((px - cx) ** 2 + (py - cy) ** 2) - r;
}

function sdfRoundedRect(px, py, rx, ry, rw, rh, cr) {
    // SDF for rounded rectangle — shrink rect by corner radius, then offset
    const cx = rx + rw / 2;
    const cy = ry + rh / 2;
    const hw = rw / 2 - cr;
    const hh = rh / 2 - cr;
    const dx = Math.max(Math.abs(px - cx) - hw, 0);
    const dy = Math.max(Math.abs(py - cy) - hh, 0);
    return Math.sqrt(dx * dx + dy * dy) - cr;
}

function sdfTriangle(px, py, x0, y0, x1, y1, x2, y2) {
    // SDF for triangle — minimum distance to any edge segment
    const d0 = distPointToSegment(px, py, x0, y0, x1, y1);
    const d1 = distPointToSegment(px, py, x1, y1, x2, y2);
    const d2 = distPointToSegment(px, py, x2, y2, x0, y0);
    // Sign: positive outside, negative inside (cross product winding)
    const sign = ((x1 - x0) * (py - y0) - (y1 - y0) * (px - x0)) >= 0 &&
                 ((x2 - x1) * (py - y1) - (y2 - y1) * (px - x1)) >= 0 &&
                 ((x0 - x2) * (py - y2) - (y0 - y2) * (px - x2)) >= 0 ? -1 : 1;
    return sign * Math.min(d0, d1, d2);
}

function distPointToSegment(px, py, x1, y1, x2, y2) {
    const dx = x2 - x1;
    const dy = y2 - y1;
    const lenSq = dx * dx + dy * dy;
    if (lenSq === 0) return Math.sqrt((px - x1) ** 2 + (py - y1) ** 2);
    let t = ((px - x1) * dx + (py - y1) * dy) / lenSq;
    t = Math.max(0, Math.min(1, t));
    const cx = x1 + t * dx;
    const cy = y1 + t * dy;
    return Math.sqrt((px - cx) ** 2 + (py - cy) ** 2);
}

// ── Clip geometry ───────────────────────────────────────────────────────────

/**
 * Clip a line segment to the interior of a circle.
 * Returns null or { x1, y1, x2, y2 } for the clipped sub-segment.
 */
function clipSegmentToCircle(sx1, sy1, sx2, sy2, cx, cy, cr) {
    // Parametric: P(t) = S1 + t*(S2-S1), t in [0,1]
    // |P(t) - C|^2 = cr^2 => quadratic in t
    const dx = sx2 - sx1;
    const dy = sy2 - sy1;
    const fx = sx1 - cx;
    const fy = sy1 - cy;

    const a = dx * dx + dy * dy;
    if (a < 1e-10) {
        // Degenerate segment (zero length)
        const d2 = fx * fx + fy * fy;
        return d2 <= cr * cr ? { x1: sx1, y1: sy1, x2: sx2, y2: sy2 } : null;
    }

    const b = 2 * (fx * dx + fy * dy);
    const c = fx * fx + fy * fy - cr * cr;
    const disc = b * b - 4 * a * c;

    if (disc < 0) {
        // No intersection with circle boundary — segment fully inside or outside
        const d2 = fx * fx + fy * fy;
        return d2 <= cr * cr ? { x1: sx1, y1: sy1, x2: sx2, y2: sy2 } : null;
    }

    const sqrtDisc = Math.sqrt(disc);
    let t0 = (-b - sqrtDisc) / (2 * a);
    let t1 = (-b + sqrtDisc) / (2 * a);

    // Clip t range to [0,1]
    t0 = Math.max(0, t0);
    t1 = Math.min(1, t1);

    if (t0 >= t1) return null; // No valid sub-segment

    return {
        x1: sx1 + t0 * dx,
        y1: sy1 + t0 * dy,
        x2: sx1 + t1 * dx,
        y2: sy1 + t1 * dy,
    };
}

/**
 * Clip an arc to the interior of a circle (flashlight).
 * Arc defined by center (acx,acy), radius ar, startAngle, sweepAngle.
 * Flashlight circle at (cx,cy) radius cr.
 *
 * Returns null or { cx, cy, r, startAngle, sweepAngle }.
 */
function clipArcToCircle(acx, acy, ar, startAngle, sweepAngle, cx, cy, cr) {
    // Distance between arc center and flashlight center
    const ddx = acx - cx;
    const ddy = acy - cy;
    const dist = Math.sqrt(ddx * ddx + ddy * ddy);

    // If arc circle entirely inside flashlight — return full arc
    if (dist + ar <= cr) {
        return { cx: acx, cy: acy, r: ar, startAngle, sweepAngle };
    }

    // If no intersection at all
    if (dist >= ar + cr || dist + cr <= ar) {
        return null;
    }

    // Two-circle intersection: find the angle range on the arc circle
    // that lies inside the flashlight circle
    // cos(alpha) = (dist^2 + ar^2 - cr^2) / (2 * dist * ar)
    const cosAlpha = (dist * dist + ar * ar - cr * cr) / (2 * dist * ar);
    const alpha = Math.acos(Math.max(-1, Math.min(1, cosAlpha)));

    // Angle from arc center to flashlight center
    const centerAngle = Math.atan2(cy - acy, cx - acx);

    // The arc's portion inside the flashlight is [centerAngle - alpha, centerAngle + alpha]
    let insideStart = centerAngle - alpha;
    let insideEnd = centerAngle + alpha;

    // Normalize the original arc range
    let arcStart = startAngle;
    let arcEnd = startAngle + sweepAngle;

    // Find overlap of [arcStart, arcEnd] with [insideStart, insideEnd]
    // accounting for angle wrapping
    const result = intersectAngleRanges(arcStart, arcEnd, insideStart, insideEnd);
    if (result === null) return null;

    return {
        cx: acx,
        cy: acy,
        r: ar,
        startAngle: result.start,
        sweepAngle: result.end - result.start,
    };
}

/**
 * Intersect two angle ranges, handling wrap-around.
 * Returns { start, end } or null.
 */
function intersectAngleRanges(aStart, aEnd, bStart, bEnd) {
    // Normalize everything to a common reference
    const TWO_PI = 2 * Math.PI;

    // Try multiple offsets to handle wrapping
    let best = null;
    for (let offset = -TWO_PI; offset <= TWO_PI; offset += TWO_PI) {
        const bs = bStart + offset;
        const be = bEnd + offset;
        const s = Math.max(aStart, bs);
        const e = Math.min(aEnd, be);
        if (e > s + 1e-6) {
            if (best === null || (e - s) > (best.end - best.start)) {
                best = { start: s, end: e };
            }
        }
    }
    return best;
}

// ── Shape generation ────────────────────────────────────────────────────────

/**
 * Generate shapes with deliberate overlap clustering.
 * @param {number} count
 * @param {number} width — container width
 * @param {number} height — container height
 * @param {number} density — 1 (spread) to 10 (pile-up)
 * @returns {Array} shapes with edges
 */
function generateShapes(count, width, height, density) {
    const shapes = [];
    // Create cluster centers — more density = fewer, tighter clusters
    const clusterCount = Math.max(2, Math.round(12 - density));
    const clusters = [];
    for (let i = 0; i < clusterCount; i++) {
        clusters.push({
            x: 80 + Math.random() * (width - 160),
            y: 80 + Math.random() * (height - 160),
        });
    }

    // Spread radius shrinks as density increases
    const spread = (width * 0.4) * (1 - (density - 1) / 12);

    for (let i = 0; i < count; i++) {
        const roll = Math.random();
        // Pick a random cluster center
        const cluster = clusters[Math.floor(Math.random() * clusterCount)];
        const ox = cluster.x + (Math.random() - 0.5) * 2 * spread;
        const oy = cluster.y + (Math.random() - 0.5) * 2 * spread;

        if (roll < 0.70) {
            // Rectangle
            const w = 40 + Math.random() * 160;
            const h = 20 + Math.random() * 100;
            const x = Math.max(0, Math.min(width - w, ox - w / 2));
            const y = Math.max(0, Math.min(height - h, oy - h / 2));
            shapes.push(makeRect(x, y, w, h));
        } else if (roll < 0.85) {
            // Rounded rectangle
            const w = 50 + Math.random() * 150;
            const h = 30 + Math.random() * 90;
            const cr = 5 + Math.random() * 15;
            const x = Math.max(0, Math.min(width - w, ox - w / 2));
            const y = Math.max(0, Math.min(height - h, oy - h / 2));
            shapes.push(makeRoundedRect(x, y, w, h, cr));
        } else if (roll < 0.95) {
            // Circle
            const r = 15 + Math.random() * 60;
            const cx = Math.max(r, Math.min(width - r, ox));
            const cy = Math.max(r, Math.min(height - r, oy));
            shapes.push(makeCircle(cx, cy, r));
        } else {
            // Triangle
            const size = 30 + Math.random() * 80;
            const cx = Math.max(size, Math.min(width - size, ox));
            const cy = Math.max(size, Math.min(height - size, oy));
            shapes.push(makeTriangle(cx, cy, size));
        }
    }

    return shapes;
}

function makeRect(x, y, w, h) {
    return {
        kind: 'rect', x, y, w, h,
        sdf: (px, py) => sdfRect(px, py, x, y, w, h),
        edges: [
            { type: 'segment', x1: x, y1: y, x2: x + w, y2: y },
            { type: 'segment', x1: x + w, y1: y, x2: x + w, y2: y + h },
            { type: 'segment', x1: x + w, y1: y + h, x2: x, y2: y + h },
            { type: 'segment', x1: x, y1: y + h, x2: x, y2: y },
        ],
    };
}

function makeRoundedRect(x, y, w, h, cr) {
    // Clamp corner radius
    cr = Math.min(cr, w / 2, h / 2);
    return {
        kind: 'rrect', x, y, w, h, cr,
        sdf: (px, py) => sdfRoundedRect(px, py, x, y, w, h, cr),
        edges: [
            // Top edge (between corners)
            { type: 'segment', x1: x + cr, y1: y, x2: x + w - cr, y2: y },
            // Right edge
            { type: 'segment', x1: x + w, y1: y + cr, x2: x + w, y2: y + h - cr },
            // Bottom edge
            { type: 'segment', x1: x + w - cr, y1: y + h, x2: x + cr, y2: y + h },
            // Left edge
            { type: 'segment', x1: x, y1: y + h - cr, x2: x, y2: y + cr },
            // Corner arcs (top-right, bottom-right, bottom-left, top-left)
            { type: 'arc', cx: x + w - cr, cy: y + cr, r: cr, startAngle: -Math.PI / 2, sweepAngle: Math.PI / 2 },
            { type: 'arc', cx: x + w - cr, cy: y + h - cr, r: cr, startAngle: 0, sweepAngle: Math.PI / 2 },
            { type: 'arc', cx: x + cr, cy: y + h - cr, r: cr, startAngle: Math.PI / 2, sweepAngle: Math.PI / 2 },
            { type: 'arc', cx: x + cr, cy: y + cr, r: cr, startAngle: Math.PI, sweepAngle: Math.PI / 2 },
        ],
    };
}

function makeCircle(cx, cy, r) {
    return {
        kind: 'circle', cx, cy, r,
        sdf: (px, py) => sdfCircle(px, py, cx, cy, r),
        edges: [
            { type: 'arc', cx, cy, r, startAngle: 0, sweepAngle: 2 * Math.PI },
        ],
    };
}

function makeTriangle(cx, cy, size) {
    // Equilateral triangle centered at (cx, cy)
    const a = size * 0.577; // circumradius ≈ size / sqrt(3)
    const x0 = cx, y0 = cy - a;
    const x1 = cx - size / 2, y1 = cy + a / 2;
    const x2 = cx + size / 2, y2 = cy + a / 2;
    return {
        kind: 'triangle', x0, y0, x1, y1, x2, y2,
        sdf: (px, py) => sdfTriangle(px, py, x0, y0, x1, y1, x2, y2),
        edges: [
            { type: 'segment', x1: x0, y1: y0, x2: x1, y2: y1 },
            { type: 'segment', x1: x1, y1: y1, x2: x2, y2: y2 },
            { type: 'segment', x1: x2, y1: y2, x2: x0, y2: y0 },
        ],
    };
}

// ── Background baking ───────────────────────────────────────────────────────

function bakeBackground(shapes, width, height) {
    const canvas = document.createElement('canvas');
    canvas.width = width;
    canvas.height = height;
    const ctx = canvas.getContext('2d');

    // Dark base
    ctx.fillStyle = '#0e0e12';
    ctx.fillRect(0, 0, width, height);

    for (const shape of shapes) {
        ctx.fillStyle = '#1a1a24';
        ctx.strokeStyle = '#2a2a3a';
        ctx.lineWidth = 1;

        if (shape.kind === 'rect') {
            ctx.fillRect(shape.x, shape.y, shape.w, shape.h);
            ctx.strokeRect(shape.x, shape.y, shape.w, shape.h);
        } else if (shape.kind === 'rrect') {
            drawRoundedRect(ctx, shape.x, shape.y, shape.w, shape.h, shape.cr);
            ctx.fill();
            ctx.stroke();
        } else if (shape.kind === 'circle') {
            ctx.beginPath();
            ctx.arc(shape.cx, shape.cy, shape.r, 0, Math.PI * 2);
            ctx.fill();
            ctx.stroke();
        } else if (shape.kind === 'triangle') {
            ctx.beginPath();
            ctx.moveTo(shape.x0, shape.y0);
            ctx.lineTo(shape.x1, shape.y1);
            ctx.lineTo(shape.x2, shape.y2);
            ctx.closePath();
            ctx.fill();
            ctx.stroke();
        }
    }

    return canvas.toDataURL();
}

function drawRoundedRect(ctx, x, y, w, h, cr) {
    ctx.beginPath();
    ctx.moveTo(x + cr, y);
    ctx.lineTo(x + w - cr, y);
    ctx.arcTo(x + w, y, x + w, y + cr, cr);
    ctx.lineTo(x + w, y + h - cr);
    ctx.arcTo(x + w, y + h, x + w - cr, y + h, cr);
    ctx.lineTo(x + cr, y + h);
    ctx.arcTo(x, y + h, x, y + h - cr, cr);
    ctx.lineTo(x, y + cr);
    ctx.arcTo(x, y, x + cr, y, cr);
    ctx.closePath();
}

// ── Test Harness ────────────────────────────────────────────────────────────

class TestHarness {
    constructor(containerId) {
        this.container = document.getElementById(containerId);
        this.shapes = [];
        this.cursorX = 0;
        this.cursorY = 0;
        this.flashlightRadius = 120;
        this.showCircle = true;
        this.running = true;

        // Pool sizes — generous for stress testing
        this.segmentPoolSize = 256;
        this.arcPoolSize = 64;

        // Create overlay (production module)
        this.overlay = new Overlay(this.container, {
            segmentPoolSize: this.segmentPoolSize,
            arcPoolSize: this.arcPoolSize,
            color: '#00ff66',
            strokeWidth: 2,
        });

        // Create flashlight circle SVG (harness only)
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

        // Perf stats
        this.perfEl = document.getElementById('perf');
        this.frameTimes = [];
        this.sdfTimes = [];
        this.clipTimes = [];
        this.svgTimes = [];
        this.segCounts = [];
        this.arcCounts = [];
        this.rollingWindow = 60;

        // Bind cursor
        this.container.addEventListener('mousemove', (e) => {
            const rect = this.container.getBoundingClientRect();
            this.cursorX = e.clientX - rect.left;
            this.cursorY = e.clientY - rect.top;
        });

        // Start rAF loop
        this._lastFrame = performance.now();
        this._raf = requestAnimationFrame((t) => this._loop(t));
    }

    regenerate(count, density) {
        const w = this.container.clientWidth;
        const h = this.container.clientHeight;
        this.shapes = generateShapes(count, w, h, density);

        // Bake background
        const dataUrl = bakeBackground(this.shapes, w, h);
        this.container.style.backgroundImage = `url(${dataUrl})`;
    }

    setFlashlightRadius(r) {
        this.flashlightRadius = r;
    }

    setShowCircle(show) {
        this.showCircle = show;
        this.flashSvg.style.display = show ? '' : 'none';
    }

    _loop(now) {
        if (!this.running) return;

        const frameStart = performance.now();
        const radius = this.flashlightRadius;
        const mx = this.cursorX;
        const my = this.cursorY;

        // Update flashlight circle
        if (this.showCircle) {
            this.flashCircle.setAttribute('cx', mx);
            this.flashCircle.setAttribute('cy', my);
            this.flashCircle.setAttribute('r', radius);
        }

        // ── SDF evaluation ──────────────────────────────────────────────
        const sdfStart = performance.now();
        const nearShapes = [];
        for (let i = 0; i < this.shapes.length; i++) {
            const shape = this.shapes[i];
            const d = shape.sdf(mx, my);
            // Shape boundary within flashlight radius (with margin for edge thickness)
            if (Math.abs(d) < radius + 10) {
                nearShapes.push(shape);
            }
        }
        const sdfTime = performance.now() - sdfStart;

        // ── Clip geometry ───────────────────────────────────────────────
        const clipStart = performance.now();
        const commands = [];

        for (let i = 0; i < nearShapes.length; i++) {
            const shape = nearShapes[i];
            const edges = shape.edges;

            for (let j = 0; j < edges.length; j++) {
                const edge = edges[j];

                if (edge.type === 'segment') {
                    const clipped = clipSegmentToCircle(
                        edge.x1, edge.y1, edge.x2, edge.y2,
                        mx, my, radius
                    );
                    if (clipped) {
                        commands.push({
                            type: 'segment',
                            x1: clipped.x1, y1: clipped.y1,
                            x2: clipped.x2, y2: clipped.y2,
                        });
                    }
                } else if (edge.type === 'arc') {
                    const clipped = clipArcToCircle(
                        edge.cx, edge.cy, edge.r,
                        edge.startAngle, edge.sweepAngle,
                        mx, my, radius
                    );
                    if (clipped) {
                        commands.push({
                            type: 'arc',
                            cx: clipped.cx, cy: clipped.cy, r: clipped.r,
                            startAngle: clipped.startAngle,
                            sweepAngle: clipped.sweepAngle,
                        });
                    }
                }
            }
        }
        const clipTime = performance.now() - clipStart;

        // ── SVG update ──────────────────────────────────────────────────
        const svgStart = performance.now();
        this.overlay.update(commands);
        const svgTime = performance.now() - svgStart;

        const frameTime = performance.now() - frameStart;

        // ── Rolling perf stats ──────────────────────────────────────────
        this.frameTimes.push(frameTime);
        this.sdfTimes.push(sdfTime);
        this.clipTimes.push(clipTime);
        this.svgTimes.push(svgTime);
        this.segCounts.push(this.overlay.activeSegments);
        this.arcCounts.push(this.overlay.activeArcs);

        if (this.frameTimes.length > this.rollingWindow) {
            this.frameTimes.shift();
            this.sdfTimes.shift();
            this.clipTimes.shift();
            this.svgTimes.shift();
            this.segCounts.shift();
            this.arcCounts.shift();
        }

        // Update display every 10 frames
        if (this.frameTimes.length % 10 === 0) {
            this._updatePerfDisplay(now);
        }

        this._lastFrame = now;
        this._raf = requestAnimationFrame((t) => this._loop(t));
    }

    _updatePerfDisplay(now) {
        const avg = (arr) => arr.reduce((a, b) => a + b, 0) / arr.length;
        const avgFrame = avg(this.frameTimes);
        const avgSdf = avg(this.sdfTimes);
        const avgClip = avg(this.clipTimes);
        const avgSvg = avg(this.svgTimes);
        const avgSegs = Math.round(avg(this.segCounts));
        const avgArcs = Math.round(avg(this.arcCounts));

        // FPS from actual frame intervals (rAF-based)
        const fps = avgFrame > 0 ? Math.min(999, 1000 / (avgFrame + (1000 / 60 - avgFrame))) : 0;
        // Better FPS: use rAF timing
        const realFps = this._fpsFromRaf(now);

        this.perfEl.textContent =
            `FPS: ${realFps.toFixed(0)} | frame: ${avgFrame.toFixed(2)}ms\n` +
            `SDF:    ${avgSdf.toFixed(2)}ms (${this.shapes.length} shapes)\n` +
            `Clip:   ${avgClip.toFixed(2)}ms (${avgSegs + avgArcs} edges)\n` +
            `SVG:    ${avgSvg.toFixed(2)}ms (${avgSegs}/${this.segmentPoolSize} lines, ${avgArcs}/${this.arcPoolSize} arcs)\n` +
            `Total:  ${(avgSdf + avgClip + avgSvg).toFixed(2)}ms`;
    }

    _fpsFromRaf(now) {
        if (!this._fpsSamples) this._fpsSamples = [];
        this._fpsSamples.push(now);
        // Keep last 61 samples (60 intervals)
        while (this._fpsSamples.length > 61) this._fpsSamples.shift();
        if (this._fpsSamples.length < 2) return 60;
        const elapsed = this._fpsSamples[this._fpsSamples.length - 1] - this._fpsSamples[0];
        return (this._fpsSamples.length - 1) / (elapsed / 1000);
    }

    destroy() {
        this.running = false;
        if (this._raf) cancelAnimationFrame(this._raf);
        this.overlay.destroy();
        if (this.flashSvg.parentNode) {
            this.flashSvg.parentNode.removeChild(this.flashSvg);
        }
    }
}
