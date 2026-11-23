// An animated desktop background: the bravebot pixel bots in a Space Invaders battle.
//
// A window per screen sits at the desktop level, under the icons and over the wallpaper, on every
// Space, and ignores the mouse. The scene is a nebula, three layers of stars, a ringed planet, a
// planet horizon, and a marching formation of bots. The orange bravebot mascot at the bottom tracks
// the formation, leads its shots, and shoots the bots down. Hit bots burst into tiles, a mothership
// crosses the top now and then, and a cleared wave comes back in new colours. Everything is drawn
// with Core Graphics and paused while the desktop is covered or the displays are asleep.
//
// A menu bar item (a bot) pauses and quits it. Options, for running the binary by hand:
//   --fps N                 frames per second (default 30)
//   --snapshot FILE.png     draw one frame to a PNG after --seconds of simulation and exit
//   --seconds N             simulated seconds before a snapshot (default 14)
//   --size WxH              snapshot size (default 1600x900)

import AppKit

// MARK: - Colour and noise

/// A colour from hue, saturation and brightness, each in 0...1 (hue wraps).
func hsb(_ hue: Double, _ saturation: Double, _ brightness: Double, _ alpha: Double = 1) -> CGColor {
    let sector = (hue - floor(hue)) * 6
    let index = Int(sector) % 6
    let fraction = sector - floor(sector)
    let low = brightness * (1 - saturation)
    let falling = brightness * (1 - saturation * fraction)
    let rising = brightness * (1 - saturation * (1 - fraction))
    let rgb: (Double, Double, Double)
    switch index {
    case 0: rgb = (brightness, rising, low)
    case 1: rgb = (falling, brightness, low)
    case 2: rgb = (low, brightness, rising)
    case 3: rgb = (low, falling, brightness)
    case 4: rgb = (rising, low, brightness)
    default: rgb = (brightness, low, falling)
    }
    return CGColor(red: rgb.0, green: rgb.1, blue: rgb.2, alpha: alpha)
}

/// A repeatable number in 0..<1 for an integer.
func noise(_ n: Int) -> Double {
    var x = UInt64(truncatingIfNeeded: n) &* 0x9E37_79B9_7F4A_7C15
    x ^= x >> 29
    x = x &* 0xBF58_476D_E5CE_4B9D
    x ^= x >> 32
    return Double(x % 1_000_000) / 1_000_000
}

func frac(_ value: Double) -> Double { value - floor(value) }

let sRGB = CGColorSpaceCreateDeviceRGB()

func gradient(_ stops: [(Double, CGColor)]) -> CGGradient {
    CGGradient(colorsSpace: sRGB, colors: stops.map { $0.1 } as CFArray, locations: stops.map { CGFloat($0.0) })!
}

// MARK: - Sprites

/// `X` is a body cell, `W` an eye (a white cell with a pupil that looks toward the hero), `L` a light.
struct Species {
    let frames: [[String]]
    let hues: (Double, Double)
}

let species: [Species] = [
    // Squid
    Species(frames: [
        ["...XX...", "..XXXX..", ".XXXXXX.", "XXWXXWXX", "XXXXXXXX", "..X..X..", ".X.XX.X.", "X.X..X.X"],
        ["...XX...", "..XXXX..", ".XXXXXX.", "XXWXXWXX", "XXXXXXXX", ".X.XX.X.", "X......X", ".X....X."],
    ], hues: (0.50, 0.72)),
    // Crab
    Species(frames: [
        ["..X..X..", "...XX...", "..XXXX..", ".XWXXWX.", "XXXXXXXX", "X.XXXX.X", "X.X..X.X", "...XX..."],
        ["..X..X..", "X..XX..X", "X.XXXX.X", "XXWXXWXX", "XXXXXXXX", ".XXXXXX.", "..X..X..", ".X....X."],
    ], hues: (0.30, 0.50)),
    // Horned
    Species(frames: [
        ["X......X", ".X....X.", ".XXXXXX.", "XXWXXWXX", "XXXXXXXX", ".XX..XX.", "XX.XX.XX", "X..XX..X"],
        ["X......X", ".X....X.", ".XXXXXX.", "XXWXXWXX", "XXXXXXXX", ".XX..XX.", ".X.XX.X.", "..X..X.."],
    ], hues: (0.88, 0.04)),
    // Octopus
    Species(frames: [
        [".XXXXXX.", "XXXXXXXX", "XXWXXWXX", "XXXXXXXX", ".XX..XX.", "XX.XX.XX", "X......X", "........"],
        [".XXXXXX.", "XXXXXXXX", "XXWXXWXX", "XXXXXXXX", "..XXXX..", ".XX..XX.", "XX....XX", "........"],
    ], hues: (0.72, 0.88)),
]

/// The orange mascot, as in the app icon: 8 by 9 cells.
let heroRows = [
    "...X....", ".X.XX.X.", "XXXXXXXX", "XXXXXXXX", "XXXXXXXX", "XXXXXXXX", "XXWXXWXX", "XXXXXXXX", ".XXXXXX.",
]

let ufoRows = ["...XXXXXX...", ".XXXXXXXXXX.", "XLXXLXXLXXLX", ".XXXXXXXXXX.", "..XX....XX.."]

let eyeWhite = CGColor(red: 1, green: 1, blue: 1, alpha: 1)
let eyePupil = CGColor(red: 0.114, green: 0.122, blue: 0.153, alpha: 1)

func drawSprite(
    _ ctx: CGContext, _ rows: [String], x: CGFloat, y: CGFloat, size: CGFloat, gaze: CGFloat,
    color: (Int, Int) -> CGColor
) {
    let gap = size * 0.05
    for (r, row) in rows.enumerated() {
        for (c, cell) in row.enumerated() where cell != "." {
            let rect = CGRect(x: x + CGFloat(c) * size, y: y + CGFloat(r) * size, width: size, height: size)
            if cell == "W" {
                ctx.setFillColor(eyeWhite)
                ctx.fill(rect)
                ctx.setFillColor(eyePupil)
                ctx.fill(CGRect(x: rect.minX + size * (0.25 + 0.25 * gaze), y: rect.minY + size * 0.3,
                                width: size * 0.5, height: size * 0.5))
            } else {
                ctx.setFillColor(color(r, c))
                ctx.fill(rect.insetBy(dx: gap, dy: gap))
            }
        }
    }
}

// MARK: - Simulation

struct Alien { var kind: Int; var row: Int; var col: Int; var alive: Bool }
struct Bullet { var x: CGFloat; var y: CGFloat }
struct Particle {
    var x: CGFloat, y: CGFloat, vx: CGFloat, vy: CGFloat
    var age: Double, life: Double, size: CGFloat, hue: Double, white: Double
}
struct Shock { var x: CGFloat, y: CGFloat, age: Double, radius: CGFloat, hue: Double }
struct Ufo { var x: CGFloat, y: CGFloat, v: CGFloat }

final class Sim {
    private(set) var width: CGFloat = 0
    private(set) var height: CGFloat = 0
    private var px: CGFloat = 7

    let columns = 10, rows = 5
    var aliens: [Alien] = []
    var originX: CGFloat = 0, originY: CGFloat = 0, direction: CGFloat = 1
    var stepClock = 0.0, frame = 0
    var palettes: [[[CGColor]]] = []

    var playerX: CGFloat = 0, playerV: CGFloat = 0, cooldown = 0.0, trailClock = 0.0, gaze: CGFloat = 1
    var bullets: [Bullet] = [], bombs: [CGPoint] = [], particles: [Particle] = [], shocks: [Shock] = []
    var bombClock = 1.0, ufoClock = 7.0, ufo: Ufo?
    var wave = 0, hueShift = 0.0, respawnClock = 0.0
    var time = 0.0, flash = 0.0, shield = 0.0

    init(width: CGFloat, height: CGFloat) { resize(width: width, height: height) }

    func resize(width: CGFloat, height: CGFloat) {
        self.width = width
        self.height = height
        px = max(3, (height / 120).rounded())
        playerX = width / 2
        bullets = []; bombs = []; particles = []; shocks = []; ufo = nil
        spawnWave(advance: false)
    }

    // Geometry
    private var pitchX: CGFloat { 12 * px }
    private var pitchY: CGFloat { 10 * px }
    private var playerY: CGFloat { height - 9 * px - height * 0.035 }
    private var bulletSpeed: CGFloat { height * 1.3 }
    private var step: CGFloat { 2 * px }
    private var aliveCount: Int { aliens.reduce(0) { $0 + ($1.alive ? 1 : 0) } }
    private var interval: Double { 0.06 + 0.5 * Double(aliveCount) / Double(columns * rows) }

    private func position(_ a: Alien) -> CGPoint {
        CGPoint(x: originX + CGFloat(a.col) * pitchX, y: originY + CGFloat(a.row) * pitchY)
    }

    private func spawnWave(advance: Bool) {
        if advance { wave += 1; hueShift += 0.11 }
        let kinds = [0, 1, 2, 3, 3]
        aliens = (0..<rows).flatMap { r in
            (0..<columns).map { c in Alien(kind: kinds[r], row: r, col: c, alive: true) }
        }
        let formation = CGFloat(columns - 1) * pitchX + 8 * px
        originX = (width - formation) / 2
        originY = height * 0.13
        direction = 1
        stepClock = 0
        palettes = species.enumerated().map { index, kind in
            (0..<8).map { r in
                (0..<8).map { c in
                    let hue = kind.hues.0 + (kind.hues.1 - kind.hues.0) * Double(r) / 7 + hueShift
                    return hsb(hue, 0.72, 0.88 + 0.1 * noise(index * 64 + r * 8 + c + wave * 7))
                }
            }
        }
    }

    // MARK: Update

    func update(_ dt: Double) {
        time += dt
        flash = max(0, flash - dt * 1.4)
        shield = max(0, shield - dt * 2)
        if respawnClock > 0 {
            respawnClock -= dt
            if respawnClock <= 0 { spawnWave(advance: true) }
        } else {
            march(dt)
            dropBombs(dt)
        }
        moveUfo(dt)
        movePlayer(dt)
        moveBullets(dt)
        moveBombs(dt)
        moveParticles(dt)
    }

    private func march(_ dt: Double) {
        stepClock += dt
        guard stepClock >= interval else { return }
        stepClock = 0
        frame ^= 1
        var minX = CGFloat.infinity, maxX = -CGFloat.infinity, maxY = -CGFloat.infinity
        for a in aliens where a.alive {
            let p = position(a)
            minX = min(minX, p.x); maxX = max(maxX, p.x + 8 * px); maxY = max(maxY, p.y + 8 * px)
        }
        let margin = width * 0.05
        if direction > 0 && maxX + step > width - margin {
            direction = -1; originY += 4 * px
        } else if direction < 0 && minX - step < margin {
            direction = 1; originY += 4 * px
        } else {
            originX += direction * step
        }
        if maxY + 4 * px > playerY - 16 * px { spawnWave(advance: false) }
    }

    private func dropBombs(_ dt: Double) {
        bombClock -= dt
        guard bombClock <= 0 else { return }
        bombClock = Double.random(in: 0.5...1.3)
        let column = Int.random(in: 0..<columns)
        if let a = aliens.filter({ $0.alive && $0.col == column }).max(by: { $0.row < $1.row }) {
            let p = position(a)
            bombs.append(CGPoint(x: p.x + 4 * px, y: p.y + 8 * px))
        }
    }

    private func moveUfo(_ dt: Double) {
        if var u = ufo {
            u.x += u.v * CGFloat(dt)
            ufo = (u.x < -14 * px || u.x > width + 2 * px) ? nil : u
            if ufo == nil { ufoClock = Double.random(in: 12...20) }
        } else {
            ufoClock -= dt
            if ufoClock <= 0 && aliveCount > 0 {
                let speed = width / 8
                ufo = Bool.random()
                    ? Ufo(x: -12 * px, y: height * 0.05, v: speed)
                    : Ufo(x: width, y: height * 0.05, v: -speed)
            }
        }
    }

    private func target() -> CGFloat? {
        if let u = ufo, u.x > 0, u.x < width - 12 * px {
            let flight = (playerY - u.y) / bulletSpeed
            return u.x + 6 * px + u.v * CGFloat(flight)
        }
        var lowest: [Int: Alien] = [:]
        for a in aliens where a.alive {
            if let held = lowest[a.col], held.row > a.row { continue }
            lowest[a.col] = a
        }
        var best: CGFloat?
        var bestDistance = CGFloat.infinity
        let drift = direction * step / CGFloat(interval)
        for a in lowest.values {
            let p = position(a)
            let flight = (playerY - p.y - 8 * px) / bulletSpeed
            let aim = p.x + 4 * px + drift * flight
            if abs(aim - playerX) < bestDistance { bestDistance = abs(aim - playerX); best = aim }
        }
        return best
    }

    private func movePlayer(_ dt: Double) {
        let aim = target()
        let desired: CGFloat
        if let aim {
            desired = max(-width * 0.6, min(width * 0.6, (aim - playerX) * 7))
            gaze = aim >= playerX ? 1 : -1
            cooldown -= dt
            if abs(aim - playerX) < 3 * px && cooldown <= 0 {
                bullets.append(Bullet(x: playerX, y: playerY - 9 * px))
                cooldown = 0.55
            }
        } else {
            desired = CGFloat(sin(time * 0.8)) * width * 0.15
        }
        playerV += (desired - playerV) * CGFloat(min(1, dt * 9))
        playerX = max(6 * px, min(width - 6 * px, playerX + playerV * CGFloat(dt)))
        trailClock -= dt
        if trailClock <= 0 {
            trailClock = 0.03
            particles.append(Particle(x: playerX + CGFloat.random(in: -2 * px...2 * px), y: playerY,
                vx: CGFloat.random(in: -20...20), vy: CGFloat.random(in: 60...160), age: 0,
                life: Double.random(in: 0.3...0.6), size: px * CGFloat.random(in: 0.5...0.9),
                hue: Double.random(in: 0.02...0.12), white: 0))
        }
    }

    private func moveBullets(_ dt: Double) {
        var kept: [Bullet] = []
        for var b in bullets {
            let previous = b.y
            b.y -= bulletSpeed * CGFloat(dt)
            var hit = false
            if let u = ufo, b.x > u.x - px, b.x < u.x + 13 * px, b.y < u.y + 5 * px, previous > u.y {
                explodeUfo(u)
                ufo = nil
                ufoClock = Double.random(in: 12...20)
                hit = true
            }
            if !hit {
                for i in aliens.indices where aliens[i].alive {
                    let p = position(aliens[i])
                    if b.x > p.x - px, b.x < p.x + 9 * px, b.y < p.y + 8 * px, previous > p.y {
                        aliens[i].alive = false
                        explodeAlien(aliens[i], at: p)
                        hit = true
                        break
                    }
                }
            }
            if !hit && b.y > -20 { kept.append(b) }
        }
        bullets = kept
        if aliveCount == 0 && respawnClock <= 0 {
            respawnClock = 1.8
            shocks.append(Shock(x: width / 2, y: height * 0.4, age: 0, radius: width * 0.6, hue: hueShift))
            flash = 0.35
        }
    }

    private func moveBombs(_ dt: Double) {
        var kept: [CGPoint] = []
        for var b in bombs {
            b.y += height * 0.3 * CGFloat(dt)
            if b.y >= playerY - 2 * px {
                burst(x: b.x, y: playerY, hue: 0.95, count: 10, power: 140, size: px * 0.6)
                if abs(b.x - playerX) < 7 * px { shield = 1 }
            } else {
                kept.append(b)
            }
        }
        bombs = kept
    }

    private func moveParticles(_ dt: Double) {
        let d = CGFloat(dt)
        particles = particles.compactMap { var p = $0
            p.age += dt
            guard p.age < p.life else { return nil }
            p.x += p.vx * d; p.y += p.vy * d
            p.vy += height * 0.25 * d
            p.vx *= 1 - 0.8 * d
            return p
        }
        shocks = shocks.compactMap { var s = $0
            s.age += dt
            return s.age < 0.9 ? s : nil
        }
        if particles.count > 1500 { particles.removeFirst(particles.count - 1500) }
    }

    // MARK: Effects

    private func burst(x: CGFloat, y: CGFloat, hue: Double, count: Int, power: CGFloat, size: CGFloat, spread: Double = 0.08) {
        for _ in 0..<count {
            let angle = Double.random(in: 0..<(2 * .pi))
            let speed = power * CGFloat.random(in: 0.2...1)
            particles.append(Particle(x: x, y: y, vx: CGFloat(cos(angle)) * speed, vy: CGFloat(sin(angle)) * speed - power * 0.3,
                age: 0, life: Double.random(in: 0.6...1.5), size: size * CGFloat.random(in: 0.5...1.2),
                hue: hue + Double.random(in: -spread...spread), white: Double.random(in: 0...0.5)))
        }
    }

    private func explodeAlien(_ a: Alien, at p: CGPoint) {
        let kind = species[a.kind]
        let hue = (kind.hues.0 + kind.hues.1) / 2 + hueShift
        burst(x: p.x + 4 * px, y: p.y + 4 * px, hue: hue, count: 34, power: height * 0.45, size: px * 0.9)
        shocks.append(Shock(x: p.x + 4 * px, y: p.y + 4 * px, age: 0, radius: 9 * px, hue: hue))
    }

    private func explodeUfo(_ u: Ufo) {
        let x = u.x + 6 * px, y = u.y + 2.5 * px
        for i in 0..<12 { burst(x: x, y: y, hue: Double(i) / 12, count: 16, power: height * 0.9, size: px, spread: 0.02) }
        for i in 0..<3 {
            shocks.append(Shock(x: x, y: y, age: -Double(i) * 0.12, radius: height * (0.35 + 0.15 * Double(i)), hue: Double(i) * 0.3 + time))
        }
        flash = 0.45
    }

    // MARK: Drawing

    func draw(_ ctx: CGContext) {
        let rect = CGRect(x: 0, y: 0, width: width, height: height)
        drawBackdrop(ctx, rect)
        drawAliens(ctx)
        drawUfo(ctx)
        drawShots(ctx)
        drawHero(ctx)
        drawEffects(ctx)
        let vignette = gradient([(0, CGColor(gray: 0, alpha: 0)), (1, CGColor(gray: 0, alpha: 0.6))])
        ctx.drawRadialGradient(vignette, startCenter: CGPoint(x: width / 2, y: height / 2), startRadius: min(width, height) * 0.45,
            endCenter: CGPoint(x: width / 2, y: height / 2), endRadius: hypot(width, height) / 2, options: .drawsAfterEndLocation)
    }

    private func drawBackdrop(_ ctx: CGContext, _ rect: CGRect) {
        let sky = gradient([(0, hsb(0.68, 0.9, 0.05)), (0.6, hsb(0.72, 0.85, 0.16)), (1, hsb(0.8, 0.8, 0.22))])
        ctx.drawLinearGradient(sky, start: .zero, end: CGPoint(x: 0, y: height), options: [])

        // Nebula: slow drifting clouds whose hue cycles.
        ctx.saveGState()
        ctx.setBlendMode(.plusLighter)
        let reach = max(width, height) * 0.5
        for i in 0..<5 {
            let f = Double(i)
            let center = CGPoint(x: width * (0.5 + 0.42 * sin(time * 0.021 + f * 1.7)), y: height * (0.45 + 0.35 * cos(time * 0.017 + f * 2.3)))
            let hue = frac(0.62 + f * 0.15 + time * 0.004 + hueShift * 0.3)
            let cloud = gradient([(0, hsb(hue, 0.85, 0.5, 0.28)), (1, hsb(hue, 0.85, 0.5, 0))])
            ctx.drawRadialGradient(cloud, startCenter: center, startRadius: 0, endCenter: center, endRadius: reach, options: [])
        }
        ctx.restoreGState()

        // Stars in three layers, drifting down at different speeds.
        let scale = px / 7
        for (layer, spec) in [(90, 6.0, 1.0), (60, 16.0, 1.6), (30, 38.0, 2.6)].enumerated() {
            let (count, speed, size) = spec
            for n in 0..<count {
                let seed = layer * 1000 + n
                let x = noise(seed * 3) * width
                let y = frac(noise(seed * 3 + 1) + time * speed * Double(scale) / Double(height)) * height
                let twinkle = 0.55 + 0.45 * sin(time * (1 + 3 * noise(seed * 3 + 2)) + Double(n))
                ctx.setFillColor(hsb(0.6 + 0.15 * noise(seed), 0.25, 1, twinkle))
                ctx.fill(CGRect(x: x, y: y, width: size * scale * 1.4, height: size * scale * 1.4))
            }
        }

        // Shooting star: one streak every few seconds.
        let slot = Int(time / 4.5)
        let phase = (time - Double(slot) * 4.5) / 1.1
        if phase < 1 {
            let start = CGPoint(x: noise(slot * 7) * width * 0.8, y: noise(slot * 7 + 1) * height * 0.35)
            let head = CGPoint(x: start.x + CGFloat(phase) * width * 0.35, y: start.y + CGFloat(phase) * height * 0.22)
            let tail = CGPoint(x: head.x - width * 0.07, y: head.y - height * 0.045)
            ctx.saveGState()
            ctx.setBlendMode(.plusLighter)
            ctx.setLineCap(.round)
            ctx.setLineWidth(max(1.5, px * 0.35))
            ctx.setStrokeColor(hsb(0.55, 0.2, 1, 1 - phase))
            ctx.move(to: tail); ctx.addLine(to: head); ctx.strokePath()
            ctx.restoreGState()
        }

        drawRingedPlanet(ctx)
        drawHorizon(ctx)
    }

    private func drawRingedPlanet(_ ctx: CGContext) {
        let center = CGPoint(x: width * 0.9, y: height * 0.6 + CGFloat(sin(time * 0.1)) * px)
        let r = height * 0.075
        func ring(frontOnly: Bool) {
            ctx.saveGState()
            ctx.translateBy(x: center.x, y: center.y)
            ctx.rotate(by: -0.35)
            if frontOnly { ctx.clip(to: CGRect(x: -r * 4, y: 0, width: r * 8, height: r * 4)) }
            ctx.scaleBy(x: 1, y: 0.28)
            let bands: [(radius: CGFloat, thickness: CGFloat)] = [(1.55, 0.16), (1.85, 0.08)]
            for (k, band) in bands.enumerated() {
                ctx.setStrokeColor(hsb(0.12 + 0.03 * Double(k), 0.35, 0.9, 0.55))
                ctx.setLineWidth(r * band.thickness)
                let reach = r * band.radius
                ctx.strokeEllipse(in: CGRect(x: -reach, y: -reach, width: 2 * reach, height: 2 * reach))
            }
            ctx.restoreGState()
        }
        ring(frontOnly: false)
        let body = gradient([(0, hsb(0.08, 0.55, 1)), (0.55, hsb(0.95, 0.7, 0.55)), (1, hsb(0.75, 0.8, 0.12))])
        ctx.saveGState()
        ctx.addEllipse(in: CGRect(x: center.x - r, y: center.y - r, width: 2 * r, height: 2 * r))
        ctx.clip()
        let light = CGPoint(x: center.x - r * 0.45, y: center.y - r * 0.4)
        ctx.drawRadialGradient(body, startCenter: light, startRadius: 0, endCenter: light, endRadius: r * 1.7, options: [])
        ctx.restoreGState()
        ring(frontOnly: true)
    }

    private func drawHorizon(_ ctx: CGContext) {
        let radius = width * 0.85
        let top = height - height * 0.085
        let center = CGPoint(x: width / 2, y: top + radius)
        let glow = gradient([(0, hsb(0.5, 0.8, 0.9, 0.7)), (0.25, hsb(0.62, 0.9, 0.7, 0.25)), (1, hsb(0.7, 0.9, 0.5, 0))])
        ctx.drawRadialGradient(glow, startCenter: center, startRadius: radius - 2, endCenter: center, endRadius: radius + height * 0.14, options: [])
        let ground = gradient([(0, hsb(0.7, 0.9, 0.2)), (1, hsb(0.7, 0.9, 0.03))])
        ctx.saveGState()
        ctx.addEllipse(in: CGRect(x: center.x - radius, y: center.y - radius, width: 2 * radius, height: 2 * radius))
        ctx.clip()
        ctx.drawLinearGradient(ground, start: CGPoint(x: 0, y: top), end: CGPoint(x: 0, y: height), options: [])
        ctx.restoreGState()
    }

    private func drawAliens(_ ctx: CGContext) {
        for a in aliens where a.alive {
            let p = position(a)
            let palette = palettes[a.kind]
            let look: CGFloat = (p.x + 4 * px) < playerX ? 1 : -1
            let pulse = 0.5 + 0.5 * sin(time * 3 + Double(a.col + a.row))
            // Soft glow under each bot.
            ctx.saveGState()
            ctx.setBlendMode(.plusLighter)
            let mid = CGPoint(x: p.x + 4 * px, y: p.y + 4 * px)
            let aura = gradient([(0, hsb(species[a.kind].hues.0 + hueShift, 0.8, 0.8, 0.18 + 0.1 * pulse)), (1, hsb(0, 0, 0, 0))])
            ctx.drawRadialGradient(aura, startCenter: mid, startRadius: 0, endCenter: mid, endRadius: 9 * px, options: [])
            ctx.restoreGState()
            drawSprite(ctx, species[a.kind].frames[frame], x: p.x, y: p.y, size: px, gaze: look) { r, c in palette[r][c] }
        }
    }

    private func drawUfo(_ ctx: CGContext) {
        guard let u = ufo else { return }
        drawSprite(ctx, ufoRows, x: u.x, y: u.y, size: px, gaze: 0) { r, c in
            if ufoRows[r][ufoRows[r].index(ufoRows[r].startIndex, offsetBy: c)] == "L" {
                return hsb(time * 2 + Double(c) * 0.1, 0.5, 1)
            }
            return hsb(0.95 + 0.04 * Double(r), 0.3, 0.85 - 0.08 * Double(r))
        }
    }

    private func drawShots(_ ctx: CGContext) {
        ctx.saveGState()
        ctx.setBlendMode(.plusLighter)
        for b in bullets {
            ctx.setFillColor(hsb(0.1, 0.9, 1, 0.3))
            ctx.fill(CGRect(x: b.x - px * 1.3, y: b.y, width: px * 2.6, height: px * 7))
            ctx.setFillColor(hsb(0.12, 0.5, 1))
            ctx.fill(CGRect(x: b.x - px * 0.5, y: b.y, width: px, height: px * 5))
        }
        for b in bombs {
            ctx.setFillColor(hsb(0.95, 0.8, 1, 0.9))
            let wobble = CGFloat(Int(time * 12) % 2) * px * 0.6
            ctx.fill(CGRect(x: b.x - px * 0.4 + wobble, y: b.y - px * 3, width: px * 0.8, height: px))
            ctx.fill(CGRect(x: b.x - px * 0.4 + px * 0.6 - wobble, y: b.y - px * 2, width: px * 0.8, height: px))
            ctx.fill(CGRect(x: b.x - px * 0.4 + wobble, y: b.y - px, width: px * 0.8, height: px))
        }
        ctx.restoreGState()
    }

    private func drawHero(_ ctx: CGContext) {
        let x = playerX - 4 * px
        let y = playerY - 9 * px + CGFloat(sin(time * 5)) * px * 0.3
        ctx.saveGState()
        ctx.setBlendMode(.plusLighter)
        let mid = CGPoint(x: playerX, y: y + 4.5 * px)
        let aura = gradient([(0, hsb(0.08, 0.9, 1, 0.35)), (1, hsb(0.08, 0.9, 1, 0))])
        ctx.drawRadialGradient(aura, startCenter: mid, startRadius: 0, endCenter: mid, endRadius: 13 * px, options: [])
        ctx.restoreGState()
        drawSprite(ctx, heroRows, x: x, y: y, size: px, gaze: gaze) { r, _ in hsb(0.09 - 0.02 * Double(r) / 8, 0.92, 1 - 0.03 * Double(r)) }
        if shield > 0 {
            ctx.setStrokeColor(hsb(0.5, 0.6, 1, shield * 0.8))
            ctx.setLineWidth(px * 0.6)
            ctx.strokeEllipse(in: CGRect(x: playerX - 8 * px, y: y - 2 * px, width: 16 * px, height: 14 * px))
        }
    }

    private func drawEffects(_ ctx: CGContext) {
        ctx.saveGState()
        ctx.setBlendMode(.plusLighter)
        for p in particles {
            let t = p.age / p.life
            let fade = 1 - t
            ctx.setFillColor(hsb(p.hue, 0.85 - 0.8 * p.white * fade, 1, fade))
            let size = p.size * CGFloat(0.4 + 0.6 * fade)
            ctx.fill(CGRect(x: p.x - size / 2, y: p.y - size / 2, width: size, height: size))
        }
        for s in shocks where s.age >= 0 {
            let t = s.age / 0.9
            let radius = s.radius * CGFloat(1 - pow(1 - t, 3))
            ctx.setStrokeColor(hsb(s.hue, 0.6, 1, pow(1 - t, 2) * 0.35))
            ctx.setLineWidth(px * CGFloat(0.4 + 1.2 * (1 - t)))
            ctx.strokeEllipse(in: CGRect(x: s.x - radius, y: s.y - radius, width: 2 * radius, height: 2 * radius))
        }
        if flash > 0 {
            ctx.setFillColor(hsb(0.6, 0.15, 1, flash * 0.5))
            ctx.fill(CGRect(x: 0, y: 0, width: width, height: height))
        }
        ctx.restoreGState()
    }
}

// MARK: - Views and windows

final class WallpaperView: NSView {
    let sim: Sim

    init(frame: NSRect, sim: Sim) {
        self.sim = sim
        super.init(frame: frame)
    }

    required init?(coder: NSCoder) { nil }

    override var isFlipped: Bool { true }
    override var isOpaque: Bool { true }

    override func setFrameSize(_ newSize: NSSize) {
        super.setFrameSize(newSize)
        if newSize.width != sim.width || newSize.height != sim.height {
            sim.resize(width: newSize.width, height: newSize.height)
        }
    }

    override func draw(_ dirtyRect: NSRect) {
        guard let ctx = NSGraphicsContext.current?.cgContext else { return }
        sim.draw(ctx)
    }
}

final class Delegate: NSObject, NSApplicationDelegate {
    private var windows: [(NSWindow, WallpaperView)] = []
    private var timer: Timer?
    private var last = ProcessInfo.processInfo.systemUptime
    private var paused = false
    private var asleep = false
    private var statusItem: NSStatusItem?
    private var pauseItem: NSMenuItem?
    let fps: Double

    init(fps: Double) { self.fps = fps }

    func applicationDidFinishLaunching(_ notification: Notification) {
        rebuildWindows()
        buildMenu()
        NotificationCenter.default.addObserver(
            self, selector: #selector(screensChanged), name: NSApplication.didChangeScreenParametersNotification, object: nil)
        let center = NSWorkspace.shared.notificationCenter
        center.addObserver(self, selector: #selector(sleep), name: NSWorkspace.screensDidSleepNotification, object: nil)
        center.addObserver(self, selector: #selector(wake), name: NSWorkspace.screensDidWakeNotification, object: nil)
        let timer = Timer(timeInterval: 1 / fps, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated { self?.tick() }
        }
        RunLoop.main.add(timer, forMode: .common)
        self.timer = timer
    }

    @objc private func screensChanged() { rebuildWindows() }
    @objc private func sleep() { asleep = true }
    @objc private func wake() { asleep = false; last = ProcessInfo.processInfo.systemUptime }

    private func rebuildWindows() {
        for (window, _) in windows { window.orderOut(nil) }
        windows = NSScreen.screens.map { screen in
            let window = NSWindow(contentRect: screen.frame, styleMask: .borderless, backing: .buffered, defer: false, screen: screen)
            window.level = NSWindow.Level(rawValue: Int(CGWindowLevelForKey(.desktopWindow)) + 1)
            window.collectionBehavior = [.canJoinAllSpaces, .stationary, .ignoresCycle, .fullScreenAuxiliary]
            window.isOpaque = true
            window.hasShadow = false
            window.ignoresMouseEvents = true
            window.isReleasedWhenClosed = false
            window.backgroundColor = .black
            let size = screen.frame.size
            let view = WallpaperView(frame: NSRect(origin: .zero, size: size), sim: Sim(width: size.width, height: size.height))
            window.contentView = view
            window.setFrame(screen.frame, display: true)
            window.orderFront(nil)
            return (window, view)
        }
    }

    private func buildMenu() {
        let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        item.button?.title = "👾"
        let menu = NSMenu()
        let pause = NSMenuItem(title: "Pause", action: #selector(togglePause), keyEquivalent: "")
        pause.target = self
        menu.addItem(pause)
        menu.addItem(.separator())
        menu.addItem(NSMenuItem(title: "Quit Alien Wallpaper", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q"))
        item.menu = menu
        statusItem = item
        pauseItem = pause
    }

    @objc private func togglePause() {
        paused.toggle()
        pauseItem?.title = paused ? "Resume" : "Pause"
        last = ProcessInfo.processInfo.systemUptime
    }

    private func tick() {
        let now = ProcessInfo.processInfo.systemUptime
        let dt = min(0.1, now - last)
        last = now
        guard !paused, !asleep else { return }
        for (window, view) in windows where window.occlusionState.contains(.visible) {
            view.sim.update(dt)
            view.needsDisplay = true
        }
    }
}

// MARK: - Entry point

let arguments = CommandLine.arguments
func option(_ name: String) -> String? {
    guard let at = arguments.firstIndex(of: name), at + 1 < arguments.count else { return nil }
    return arguments[at + 1]
}

let application = NSApplication.shared

if let path = option("--snapshot") {
    let parts = (option("--size") ?? "1600x900").split(separator: "x").compactMap { Double($0) }
    let size = CGSize(width: parts.first ?? 1600, height: parts.count > 1 ? parts[1] : 900)
    let sim = Sim(width: size.width, height: size.height)
    let seconds = Double(option("--seconds") ?? "") ?? 14
    for _ in 0..<Int(seconds * 30) { sim.update(1.0 / 30) }
    let view = WallpaperView(frame: NSRect(origin: .zero, size: size), sim: sim)
    guard let rep = view.bitmapImageRepForCachingDisplay(in: view.bounds) else { exit(1) }
    view.cacheDisplay(in: view.bounds, to: rep)
    guard let png = rep.representation(using: .png, properties: [:]) else { exit(1) }
    do { try png.write(to: URL(fileURLWithPath: path)) } catch { FileHandle.standardError.write(Data("\(error)\n".utf8)); exit(1) }
    exit(0)
}

application.setActivationPolicy(.accessory)
let delegate = Delegate(fps: max(5, min(120, Double(option("--fps") ?? "") ?? 30)))
application.delegate = delegate
application.run()
