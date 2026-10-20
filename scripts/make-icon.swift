// Turns the brand artwork into a macOS app icon set.
//
//   swiftc scripts/make-icon.swift -o build/make-icon
//   build/make-icon assets/brand/logo.png build/AppIcon.iconset assets/brand/edgeway-icon.png
//
// The source is the full logo on a black background. This finds the icon
// shape, cuts it out with a continuous-corner mask so no background shows,
// places it on Apple's 1024-point grid (824-point body, soft shadow) and
// writes every size `iconutil` needs.
//
// Copyright (c) 2026 Rafe Uddaraj. MIT License.

import AppKit

func fail(_ message: String) -> Never {
    FileHandle.standardError.write(Data("make-icon: \(message)\n".utf8))
    exit(1)
}

let args = CommandLine.arguments
guard args.count >= 3 else { fail("usage: make-icon <logo.png> <out.iconset> [master.png]") }
guard let source = NSImage(contentsOfFile: args[1]),
      let cg = source.cgImage(forProposedRect: nil, context: nil, hints: nil) else { fail("cannot read \(args[1])") }

// 1. Find the icon body: the bounding box of pixels clearly brighter than the
//    black backdrop.
let width = cg.width, height = cg.height
var pixels = [UInt8](repeating: 0, count: width * height * 4)
let space = CGColorSpaceCreateDeviceRGB()
guard let scan = CGContext(data: &pixels, width: width, height: height, bitsPerComponent: 8, bytesPerRow: width * 4,
                           space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { fail("no context") }
scan.draw(cg, in: CGRect(x: 0, y: 0, width: width, height: height))
var minX = width, minY = height, maxX = 0, maxY = 0
for y in 0..<height {
    for x in 0..<width {
        let i = (y * width + x) * 4
        let brightness = max(pixels[i], pixels[i + 1], pixels[i + 2])
        if brightness > 40 {
            minX = min(minX, x); maxX = max(maxX, x)
            minY = min(minY, y); maxY = max(maxY, y)
        }
    }
}
guard maxX > minX, maxY > minY else { fail("could not find the icon in the artwork") }
// Pixel rows in the bitmap run top-down; CGImage cropping uses the same.
let side = max(maxX - minX, maxY - minY) + 1
let body = CGRect(x: minX, y: height - 1 - maxY, width: side, height: side)
guard let cropped = cg.cropping(to: body) else { fail("crop failed") }

/// Apple-style continuous corner ("squircle") path.
func squircle(in r: CGRect) -> CGPath {
    let path = CGMutablePath()
    let n: CGFloat = 5
    let steps = 720
    for i in 0..<steps {
        let t = CGFloat(i) / CGFloat(steps) * 2 * .pi
        let c = cos(t), s = sin(t)
        let x = pow(abs(c), 2 / n) * (c < 0 ? -1 : 1)
        let y = pow(abs(s), 2 / n) * (s < 0 ? -1 : 1)
        let p = CGPoint(x: r.midX + x * r.width / 2, y: r.midY + y * r.height / 2)
        i == 0 ? path.move(to: p) : path.addLine(to: p)
    }
    path.closeSubpath()
    return path
}

// 2. Render the 1024 master: 824-point body centred, masked, with shadow.
func render(size: Int) -> CGImage {
    let s = CGFloat(size)
    guard let ctx = CGContext(data: nil, width: size, height: size, bitsPerComponent: 8, bytesPerRow: 0, space: space,
                              bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { fail("no context") }
    ctx.interpolationQuality = .high
    let scale = s / 1024
    let bodyRect = CGRect(x: 100 * scale, y: 100 * scale, width: 824 * scale, height: 824 * scale)
    // Slight inset so none of the source's black corners can survive.
    let mask = squircle(in: bodyRect.insetBy(dx: bodyRect.width * 0.006, dy: bodyRect.height * 0.006))

    ctx.saveGState()
    ctx.setShadow(offset: CGSize(width: 0, height: -10 * scale), blur: 22 * scale,
                  color: CGColor(gray: 0, alpha: 0.32))
    ctx.addPath(mask)
    ctx.setFillColor(CGColor(red: 0.08, green: 0.15, blue: 0.62, alpha: 1))
    ctx.fillPath()
    ctx.restoreGState()

    ctx.saveGState()
    ctx.addPath(mask)
    ctx.clip()
    // The source squircle fills its crop; scale it slightly past the mask.
    ctx.draw(cropped, in: bodyRect.insetBy(dx: -bodyRect.width * 0.004, dy: -bodyRect.height * 0.004))
    ctx.restoreGState()
    guard let image = ctx.makeImage() else { fail("render failed") }
    return image
}

func write(_ image: CGImage, to path: String) {
    let rep = NSBitmapImageRep(cgImage: image)
    guard let data = rep.representation(using: .png, properties: [:]) else { fail("png encode failed") }
    do { try data.write(to: URL(fileURLWithPath: path)) } catch { fail("cannot write \(path)") }
}

// 3. Every size iconutil expects.
let iconset = args[2]
try? FileManager.default.createDirectory(atPath: iconset, withIntermediateDirectories: true)
for base in [16, 32, 128, 256, 512] {
    write(render(size: base), to: "\(iconset)/icon_\(base)x\(base).png")
    write(render(size: base * 2), to: "\(iconset)/icon_\(base)x\(base)@2x.png")
}
if args.count >= 4 {
    write(render(size: 1024), to: args[3])
}
print("make-icon: wrote \(iconset)")
