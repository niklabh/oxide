document.addEventListener('DOMContentLoaded', () => {
    initParticles();
    initNavbar();
    initScrollAnimations();
    initCopyCA();
    initMobileMenu();
    initVideoPlay();
    initForgePipeline();
});

/* ─── Particle Background ─── */
function initParticles() {
    const canvas = document.getElementById('particles-canvas');
    const ctx = canvas.getContext('2d');
    let particles = [];
    let mouse = { x: null, y: null };
    let animationId;

    function resize() {
        canvas.width = window.innerWidth;
        canvas.height = window.innerHeight;
    }

    resize();
    window.addEventListener('resize', resize);
    window.addEventListener('mousemove', (e) => {
        mouse.x = e.clientX;
        mouse.y = e.clientY;
    });

    class Particle {
        constructor() {
            this.reset();
        }

        reset() {
            this.x = Math.random() * canvas.width;
            this.y = Math.random() * canvas.height;
            this.size = Math.random() * 1.5 + 0.5;
            this.speedX = (Math.random() - 0.5) * 0.3;
            this.speedY = (Math.random() - 0.5) * 0.3;
            this.opacity = Math.random() * 0.5 + 0.1;
        }

        update() {
            this.x += this.speedX;
            this.y += this.speedY;

            if (mouse.x !== null) {
                const dx = mouse.x - this.x;
                const dy = mouse.y - this.y;
                const dist = Math.sqrt(dx * dx + dy * dy);
                if (dist < 150) {
                    const force = (150 - dist) / 150;
                    this.x -= (dx / dist) * force * 0.5;
                    this.y -= (dy / dist) * force * 0.5;
                }
            }

            if (this.x < 0 || this.x > canvas.width) this.speedX *= -1;
            if (this.y < 0 || this.y > canvas.height) this.speedY *= -1;
        }

        draw() {
            ctx.beginPath();
            ctx.arc(this.x, this.y, this.size, 0, Math.PI * 2);
            ctx.fillStyle = `rgba(108, 92, 231, ${this.opacity})`;
            ctx.fill();
        }
    }

    const count = Math.min(80, Math.floor((canvas.width * canvas.height) / 15000));
    for (let i = 0; i < count; i++) {
        particles.push(new Particle());
    }

    function connectParticles() {
        for (let i = 0; i < particles.length; i++) {
            for (let j = i + 1; j < particles.length; j++) {
                const dx = particles[i].x - particles[j].x;
                const dy = particles[i].y - particles[j].y;
                const dist = Math.sqrt(dx * dx + dy * dy);
                if (dist < 120) {
                    const opacity = (1 - dist / 120) * 0.15;
                    ctx.beginPath();
                    ctx.moveTo(particles[i].x, particles[i].y);
                    ctx.lineTo(particles[j].x, particles[j].y);
                    ctx.strokeStyle = `rgba(108, 92, 231, ${opacity})`;
                    ctx.lineWidth = 0.5;
                    ctx.stroke();
                }
            }
        }
    }

    function animate() {
        ctx.clearRect(0, 0, canvas.width, canvas.height);
        particles.forEach(p => {
            p.update();
            p.draw();
        });
        connectParticles();
        animationId = requestAnimationFrame(animate);
    }

    animate();
}

/* ─── Navbar Scroll ─── */
function initNavbar() {
    const navbar = document.getElementById('navbar');
    let lastScroll = 0;

    window.addEventListener('scroll', () => {
        const current = window.scrollY;
        if (current > 50) {
            navbar.classList.add('scrolled');
        } else {
            navbar.classList.remove('scrolled');
        }
        lastScroll = current;
    }, { passive: true });
}

/* ─── Scroll Animations ─── */
function initScrollAnimations() {
    const elements = document.querySelectorAll('[data-animate]');

    const observer = new IntersectionObserver((entries) => {
        entries.forEach(entry => {
            if (entry.isIntersecting) {
                const delay = entry.target.dataset.delay || 0;
                setTimeout(() => {
                    entry.target.classList.add('visible');
                }, parseInt(delay));
                observer.unobserve(entry.target);
            }
        });
    }, {
        threshold: 0.1,
        rootMargin: '0px 0px -40px 0px'
    });

    elements.forEach(el => observer.observe(el));
}

/* ─── Copy Contract Address ─── */
function initCopyCA() {
    const btn = document.getElementById('copy-ca');
    const text = document.getElementById('ca-text');
    if (!btn || !text) return;

    btn.addEventListener('click', async () => {
        try {
            await navigator.clipboard.writeText(text.textContent.trim());
            btn.classList.add('copied');
            btn.querySelector('.copy-label').textContent = 'Copied!';
            setTimeout(() => {
                btn.classList.remove('copied');
                btn.querySelector('.copy-label').textContent = 'Copy';
            }, 2000);
        } catch {
            const range = document.createRange();
            range.selectNodeContents(text);
            const sel = window.getSelection();
            sel.removeAllRanges();
            sel.addRange(range);
            document.execCommand('copy');
            sel.removeAllRanges();
            btn.classList.add('copied');
            btn.querySelector('.copy-label').textContent = 'Copied!';
            setTimeout(() => {
                btn.classList.remove('copied');
                btn.querySelector('.copy-label').textContent = 'Copy';
            }, 2000);
        }
    });
}

/* ─── Video Play Overlay ─── */
function initVideoPlay() {
    document.querySelectorAll('.video-stage').forEach(stage => {
        const video = stage.querySelector('video');
        const button = stage.querySelector('.video-play-overlay');
        if (!video || !button) return;

        button.addEventListener('click', () => {
            video.play();
        });

        video.addEventListener('play', () => {
            stage.classList.add('is-playing');
        }, { once: true });
    });
}

/* ─── Forge Pipeline (scene-6 port) ─── */
function initForgePipeline() {
    const root = document.getElementById('forge-pipeline');
    if (!root) return;

    const typedEl    = document.getElementById('forge-typed');
    const codeLines  = root.querySelectorAll('#forge-code .ln');
    const buildLines = root.querySelectorAll('#forge-build .ln');
    const tabEl      = document.getElementById('forge-tab');
    const canvasEl   = document.getElementById('forge-tab-canvas');

    if (!typedEl || !codeLines.length || !buildLines.length || !tabEl || !canvasEl) return;

    const PROMPT = "Conway's Game of Life on an 80x60 grid — phosphor green cells, purple grid, drag to paint.";

    function mulberry32(seed) {
        return function () {
            seed |= 0;
            seed = (seed + 0x6d2b79f5) | 0;
            let t = seed;
            t = Math.imul(t ^ (t >>> 15), t | 1);
            t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
            return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
        };
    }

    /* ── Game of Life state ── */
    let lifeCols = 0, lifeRows = 0, cellW = 0, cellH = 0;
    let cellElements = [];
    let cellState = null;
    let nextState = null;
    let lifeInterval = null;

    function buildLifeCanvas() {
        canvasEl.innerHTML = '';
        cellElements = [];
        const canvasH = canvasEl.clientHeight || 150;
        const canvasW = canvasEl.clientWidth || 280;
        const cellSize = Math.max(8, Math.floor(canvasH / 14));
        const cols = Math.max(8, Math.floor(canvasW / cellSize));
        const rows = Math.max(6, Math.floor(canvasH / cellSize));

        for (let i = 1; i < cols; i += 4) {
            const ln = document.createElement('div');
            ln.className = 'forge-life-grid-line';
            ln.style.left = i * cellSize + 'px';
            ln.style.top = '0';
            ln.style.width = '1px';
            ln.style.height = (rows * cellSize) + 'px';
            canvasEl.appendChild(ln);
        }
        for (let j = 1; j < rows; j += 3) {
            const ln = document.createElement('div');
            ln.className = 'forge-life-grid-line';
            ln.style.top = j * cellSize + 'px';
            ln.style.left = '0';
            ln.style.height = '1px';
            ln.style.width = (cols * cellSize) + 'px';
            canvasEl.appendChild(ln);
        }

        cellElements = new Array(cols * rows);
        for (let j = 0; j < rows; j++) {
            for (let i = 0; i < cols; i++) {
                const c = document.createElement('div');
                c.className = 'forge-life-cell';
                c.style.left = (i * cellSize + 1) + 'px';
                c.style.top  = (j * cellSize + 1) + 'px';
                c.style.width  = Math.max(1, cellSize - 2) + 'px';
                c.style.height = Math.max(1, cellSize - 2) + 'px';
                canvasEl.appendChild(c);
                cellElements[j * cols + i] = c;
            }
        }

        lifeCols = cols;
        lifeRows = rows;
        cellW = cellSize;
        cellH = cellSize;
        cellState = new Uint8Array(cols * rows);
        nextState = new Uint8Array(cols * rows);
        seedGrid(Date.now() & 0xffffffff);
        renderCells();
    }

    function seedGrid(seed) {
        const rng = mulberry32(seed);
        for (let k = 0; k < cellState.length; k++) {
            cellState[k] = rng() < 0.32 ? 1 : 0;
        }
    }

    function renderCells() {
        for (let k = 0; k < cellState.length; k++) {
            cellElements[k].style.opacity = cellState[k] ? '0.9' : '0';
        }
    }

    function stepLife() {
        const cols = lifeCols, rows = lifeRows;
        const cur = cellState, nxt = nextState;
        for (let j = 0; j < rows; j++) {
            const jUp = (j - 1 + rows) % rows;
            const jDn = (j + 1) % rows;
            for (let i = 0; i < cols; i++) {
                const iLt = (i - 1 + cols) % cols;
                const iRt = (i + 1) % cols;
                const n =
                    cur[jUp * cols + iLt] + cur[jUp * cols + i] + cur[jUp * cols + iRt] +
                    cur[j   * cols + iLt]                       + cur[j   * cols + iRt] +
                    cur[jDn * cols + iLt] + cur[jDn * cols + i] + cur[jDn * cols + iRt];
                const alive = cur[j * cols + i];
                nxt[j * cols + i] = (alive ? (n === 2 || n === 3) : (n === 3)) ? 1 : 0;
            }
        }
        cellState = nxt;
        nextState = cur;
        renderCells();
    }

    function startLifeStepper() {
        if (lifeInterval) return;
        lifeInterval = setInterval(stepLife, 360);
    }

    function stopLifeStepper() {
        if (lifeInterval) {
            clearInterval(lifeInterval);
            lifeInterval = null;
        }
    }

    /* ── Drag-to-paint ── */
    let painting = false;

    function paintAt(clientX, clientY) {
        if (!cellState) return;
        const rect = canvasEl.getBoundingClientRect();
        const x = clientX - rect.left;
        const y = clientY - rect.top;
        const i = Math.floor(x / cellW);
        const j = Math.floor(y / cellH);
        if (i < 0 || i >= lifeCols || j < 0 || j >= lifeRows) return;
        for (let dj = 0; dj < 2; dj++) {
            for (let di = 0; di < 2; di++) {
                const ni = i + di, nj = j + dj;
                if (ni >= lifeCols || nj >= lifeRows) continue;
                const k = nj * lifeCols + ni;
                if (!cellState[k]) {
                    cellState[k] = 1;
                    cellElements[k].style.opacity = '0.9';
                }
            }
        }
    }

    canvasEl.addEventListener('mousedown', (e) => {
        painting = true;
        paintAt(e.clientX, e.clientY);
    });
    canvasEl.addEventListener('mousemove', (e) => {
        if (painting) paintAt(e.clientX, e.clientY);
    });
    window.addEventListener('mouseup', () => { painting = false; });
    canvasEl.addEventListener('mouseleave', () => { painting = false; });
    canvasEl.addEventListener('touchstart', (e) => {
        if (!e.touches.length) return;
        e.preventDefault();
        painting = true;
        paintAt(e.touches[0].clientX, e.touches[0].clientY);
    }, { passive: false });
    canvasEl.addEventListener('touchmove', (e) => {
        if (!painting || !e.touches.length) return;
        e.preventDefault();
        paintAt(e.touches[0].clientX, e.touches[0].clientY);
    }, { passive: false });
    canvasEl.addEventListener('touchend', () => { painting = false; });

    /* ── Cycle: type prompt → stream code → build log → tab opens (once) ── */
    let timers = [];
    let running = false;

    function clearTimers() {
        timers.forEach(t => clearTimeout(t));
        timers = [];
    }

    function at(ms, fn) {
        timers.push(setTimeout(fn, ms));
    }

    function typePrompt(durMs) {
        const start = performance.now();
        function tick(now) {
            if (!running) return;
            const t = Math.min(1, (now - start) / durMs);
            typedEl.textContent = PROMPT.slice(0, Math.round(PROMPT.length * t));
            if (t < 1) requestAnimationFrame(tick);
        }
        requestAnimationFrame(tick);
    }

    function runCycle() {
        if (!running) return;

        typedEl.textContent = '';
        codeLines.forEach(ln => ln.classList.remove('show'));
        buildLines.forEach(ln => ln.classList.remove('show'));

        const promptDur = 3200;
        typePrompt(promptDur);

        const codeStart = promptDur + 250;
        codeLines.forEach((ln, i) => {
            at(codeStart + i * 140, () => ln.classList.add('show'));
        });

        const buildStart = codeStart + codeLines.length * 140 + 300;
        buildLines.forEach((ln, i) => {
            at(buildStart + i * 380, () => ln.classList.add('show'));
        });

        const tabOpen = buildStart + buildLines.length * 380 + 200;
        at(tabOpen, () => {
            if (!tabEl.classList.contains('show')) {
                tabEl.classList.add('show');
            }
            startLifeStepper();
        });

        at(tabOpen + 8000, runCycle);
    }

    function start() {
        if (running) return;
        running = true;
        buildLifeCanvas();
        runCycle();
    }

    function stop() {
        running = false;
        clearTimers();
        stopLifeStepper();
    }

    const io = new IntersectionObserver((entries) => {
        entries.forEach(e => {
            if (e.isIntersecting) start();
            else stop();
        });
    }, { threshold: 0.2 });
    io.observe(root);

    let resizeT;
    window.addEventListener('resize', () => {
        clearTimeout(resizeT);
        resizeT = setTimeout(() => {
            if (running) {
                stop();
                start();
            }
        }, 200);
    });
}

/* ─── Mobile Menu ─── */
function initMobileMenu() {
    const toggle = document.getElementById('mobile-toggle');
    const links = document.getElementById('nav-links');
    if (!toggle || !links) return;

    toggle.addEventListener('click', () => {
        links.classList.toggle('open');
        toggle.classList.toggle('active');
    });

    links.querySelectorAll('a').forEach(link => {
        link.addEventListener('click', () => {
            links.classList.remove('open');
            toggle.classList.remove('active');
        });
    });
}
