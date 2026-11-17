// SPDX-License-Identifier: MIT OR Apache-2.0
// Asset-authoring utility; the Cueglint app does not invoke Swift or AppKit.
import AppKit
import Foundation

let directory = URL(fileURLWithPath: CommandLine.arguments.dropFirst().first ?? "ui/brand", isDirectory: true)
let teal = CGColor(srgbRed: 149 / 255, green: 220 / 255, blue: 204 / 255, alpha: 1)
let dark = CGColor(srgbRed: 44 / 255, green: 40 / 255, blue: 41 / 255, alpha: 1)
let light = CGColor(srgbRed: 244 / 255, green: 242 / 255, blue: 243 / 255, alpha: 1)
let curve = "M39 12H28C16.954305 12 8 20.954305 8 32C8 43.045695 16.954305 52 28 52H39"
let vertical = "47,10 52,24 47,38 42,24"
let horizontal = "34,24 47,19 60,24 47,29"
let letters = [
    "M23 25C20 22 17 21 13 21C6 21 2 27 2 36C2 45 6 51 13 51C17 51 21 49 23 47",
    "M32 22V40C32 47 36 51 43 51C50 51 55 46 55 40M55 22V51",
    "M67 36H90C90 27 86 21 79 21C71 21 66 27 66 36C66 45 71 51 79 51C83 51 87 49 89 47",
    "M125 22V50C125 58 121 63 113 63C109 63 106 62 103 60M125 35.5C125 26 121 21 113 21C105.5 21 101 27 101 35.5C101 44 105.5 50 113 50C120 50 125 44 125 35.5",
    "M138 11V44C138 49 140 51 145 51",
    "M158 22V51",
    "M172 51V22M172 32C172 25 177 21 184 21C191 21 196 25 196 33V51",
    "M210 13V42C210 48 213 51 219 51C221 51 223 50 225 49M202 22H223",
]

func markSVG(_ color: String) -> String {
    "<g fill=\"\(color)\"><path d=\"\(curve)\" fill=\"none\" stroke=\"\(color)\" stroke-width=\"8\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/><polygon points=\"\(vertical)\"/><polygon points=\"\(horizontal)\"/></g>"
}

func svg(_ name: String, width: Int, height: Int, body: String) throws {
    let xml = """
    <!-- SPDX-License-Identifier: MIT OR Apache-2.0; original Cueglint artwork. -->
    <svg xmlns="http://www.w3.org/2000/svg" width="\(width)" height="\(height)" viewBox="0 0 \(width) \(height)" role="img" aria-labelledby="title">
      <title id="title">Cueglint</title>
      \(body)
    </svg>

    """
    try xml.write(to: directory.appendingPathComponent(name), atomically: true, encoding: .utf8)
}

// Only the SVG commands used by this authored artwork are needed for the PNG exports.
func path(_ source: String) -> CGPath {
    let scanner = Scanner(string: source.replacingOccurrences(of: ",", with: " "))
    scanner.charactersToBeSkipped = .whitespacesAndNewlines
    let result = CGMutablePath()
    var command = ""
    var current = CGPoint.zero
    func number() -> CGFloat {
        guard let value = scanner.scanDouble() else { preconditionFailure("Invalid authored path") }
        return CGFloat(value)
    }
    while !scanner.isAtEnd {
        if let next = scanner.scanCharacters(from: .letters) { command = next }
        switch command {
        case "M": current = CGPoint(x: number(), y: number()); result.move(to: current)
        case "H": current.x = number(); result.addLine(to: current)
        case "V": current.y = number(); result.addLine(to: current)
        case "C":
            let first = CGPoint(x: number(), y: number())
            let second = CGPoint(x: number(), y: number())
            current = CGPoint(x: number(), y: number())
            result.addCurve(to: current, control1: first, control2: second)
        default: preconditionFailure("Unsupported authored path command")
        }
    }
    return result
}

func polygon(_ context: CGContext, _ points: [(CGFloat, CGFloat)]) {
    context.beginPath()
    context.move(to: CGPoint(x: points[0].0, y: points[0].1))
    for point in points.dropFirst() { context.addLine(to: CGPoint(x: point.0, y: point.1)) }
    context.closePath()
    context.fillPath()
}

func mark(_ context: CGContext, color: CGColor = teal) {
    context.setStrokeColor(color)
    context.setFillColor(color)
    context.setLineWidth(8)
    context.setLineCap(.round)
    context.setLineJoin(.round)
    context.addPath(path(curve))
    context.strokePath()
    polygon(context, [(47, 10), (52, 24), (47, 38), (42, 24)])
    polygon(context, [(34, 24), (47, 19), (60, 24), (47, 29)])
}

func wordmark(_ context: CGContext, color: CGColor) {
    context.setStrokeColor(color)
    context.setLineWidth(4.4)
    context.setLineCap(.round)
    context.setLineJoin(.round)
    for letter in letters { context.addPath(path(letter)); context.strokePath() }
    context.setFillColor(color)
    context.fillEllipse(in: CGRect(x: 155.4, y: 8.4, width: 5.2, height: 5.2))
}

func logo(_ context: CGContext, color: CGColor) {
    context.saveGState()
    context.translateBy(x: 4, y: 8)
    mark(context)
    context.restoreGState()
    context.saveGState()
    context.translateBy(x: 90, y: 8)
    wordmark(context, color: color)
    context.restoreGState()
}

func appIcon(_ context: CGContext) {
    context.setFillColor(dark)
    context.addPath(CGPath(roundedRect: CGRect(x: 8, y: 8, width: 240, height: 240), cornerWidth: 54, cornerHeight: 54, transform: nil))
    context.fillPath()
    context.saveGState()
    context.translateBy(x: 44, y: 44)
    context.scaleBy(x: 2.625, y: 2.625)
    mark(context)
    context.restoreGState()
}

@discardableResult
func png(_ name: String, width: Int, height: Int, scale: Int = 1, output: URL = directory, draw: (CGContext) -> Void) throws -> CGImage {
    guard let context = CGContext(data: nil, width: width * scale, height: height * scale, bitsPerComponent: 8, bytesPerRow: width * scale * 4, space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { fatalError("Cannot allocate export") }
    context.translateBy(x: 0, y: CGFloat(height * scale))
    context.scaleBy(x: CGFloat(scale), y: -CGFloat(scale))
    draw(context)
    let image = context.makeImage()!
    let bitmap = NSBitmapImageRep(cgImage: image)
    let data = bitmap.representation(using: .png, properties: [:])!
    try data.write(to: output.appendingPathComponent(name))
    return image
}

func label(_ text: String, at point: CGPoint, in context: CGContext, size: CGFloat, color: CGColor) {
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(cgContext: context, flipped: true)
    (text as NSString).draw(at: point, withAttributes: [.font: NSFont.systemFont(ofSize: size, weight: .medium), .foregroundColor: NSColor(cgColor: color)!])
    NSGraphicsContext.restoreGraphicsState()
}

try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
try svg("cueglint-mark.svg", width: 64, height: 64, body: markSVG("#95dccc"))
try svg("cueglint-mark-mono.svg", width: 64, height: 64, body: markSVG("#2c2829"))
try svg("cueglint-mark-inverse.svg", width: 64, height: 64, body: markSVG("#f4f2f3"))
for (name, color) in [("cueglint-logo", "#2c2829"), ("cueglint-logo-inverse", "#f4f2f3")] {
    let strokes = letters.map { "<path d=\"\($0)\"/>" }.joined()
    try svg("\(name).svg", width: 332, height: 80, body: "<g transform=\"translate(4 8)\">\(markSVG("#95dccc"))</g><g transform=\"translate(90 8)\" stroke=\"\(color)\" stroke-width=\"4.4\" stroke-linecap=\"round\" stroke-linejoin=\"round\" fill=\"none\">\(strokes)<circle cx=\"158\" cy=\"11\" r=\"2.6\" fill=\"\(color)\" stroke=\"none\"/></g>")
    try png("\(name).png", width: 332, height: 80, scale: 4) { logo($0, color: color == "#2c2829" ? dark : light) }
}
try svg("cueglint-app-icon.svg", width: 256, height: 256, body: "<rect x=\"8\" y=\"8\" width=\"240\" height=\"240\" rx=\"54\" fill=\"#2c2829\"/><g transform=\"translate(44 44) scale(2.625)\">\(markSVG("#95dccc"))</g>")
for size in [16, 24, 32, 64, 128, 256, 512] {
    try png("cueglint-mark-\(size).png", width: size, height: size) { context in
        context.scaleBy(x: CGFloat(size) / 64, y: CGFloat(size) / 64)
        mark(context)
    }
}
try png("cueglint-mark-mono.png", width: 64, height: 64, scale: 4) { mark($0, color: dark) }
try png("cueglint-mark-inverse.png", width: 64, height: 64, scale: 4) { mark($0, color: light) }
for scale in [1, 4] { try png("cueglint-app-icon-\(256 * scale).png", width: 256, height: 256, scale: scale, draw: appIcon) }

let iconset = FileManager.default.temporaryDirectory.appendingPathComponent("cueglint-\(UUID().uuidString).iconset", isDirectory: true)
try FileManager.default.createDirectory(at: iconset, withIntermediateDirectories: true)
defer { try? FileManager.default.removeItem(at: iconset) }
for size in [16, 32, 128, 256, 512] {
    for factor in [1, 2] {
        let pixels = size * factor
        let name = "icon_\(size)x\(size)\(factor == 2 ? "@2x" : "").png"
        try png(name, width: pixels, height: pixels, output: iconset) { context in
            context.scaleBy(x: CGFloat(pixels) / 256, y: CGFloat(pixels) / 256)
            appIcon(context)
        }
    }
}
let iconutil = Process()
iconutil.executableURL = URL(fileURLWithPath: "/usr/bin/iconutil")
iconutil.arguments = ["--convert", "icns", "--output", directory.appendingPathComponent("Cueglint.icns").path, iconset.path]
try iconutil.run()
iconutil.waitUntilExit()
precondition(iconutil.terminationStatus == 0, "iconutil failed")

try png("preview.png", width: 1200, height: 760) { context in
    context.setFillColor(light); context.fill(CGRect(x: 0, y: 0, width: 600, height: 760))
    context.setFillColor(dark); context.fill(CGRect(x: 600, y: 0, width: 600, height: 760))
    for (left, ink) in [(CGFloat(0), dark), (CGFloat(600), light)] {
        label("CUEGLINT / ORIGINAL IDENTITY", at: CGPoint(x: left + 48, y: 40), in: context, size: 13, color: ink)
        context.saveGState(); context.translateBy(x: left + 44, y: 116); context.scaleBy(x: 1.5, y: 1.5); logo(context, color: ink); context.restoreGState()
        label("Native mark · actual logical sizes", at: CGPoint(x: left + 48, y: 296), in: context, size: 14, color: ink)
        for (index, size) in [16, 24, 32, 64].enumerated() {
            let x = left + 48 + CGFloat(index) * 124
            context.saveGState(); context.translateBy(x: x, y: 354); context.scaleBy(x: CGFloat(size) / 64, y: CGFloat(size) / 64); mark(context, color: ink); context.restoreGState()
            label("\(size) px", at: CGPoint(x: x, y: 436), in: context, size: 12, color: ink)
        }
        context.saveGState(); context.translateBy(x: left + 40, y: 526); context.scaleBy(x: 0.625, y: 0.625); appIcon(context); context.restoreGState()
        label("Context, with a glint.", at: CGPoint(x: left + 236, y: 556), in: context, size: 19, color: ink)
        label("#95DCCC  ·  #2C2829  ·  #F4F2F3", at: CGPoint(x: left + 236, y: 594), in: context, size: 12, color: ink)
        label("Original vectors. Transparent exports.", at: CGPoint(x: left + 236, y: 624), in: context, size: 12, color: ink)
    }
}
print("Exported original Cueglint assets to \(directory.path)")
