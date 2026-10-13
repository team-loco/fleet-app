import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

let canvasSize = 1024
let artworkSize: CGFloat = 824
let cornerRatio: CGFloat = 0.22
let cornerExponent: CGFloat = 2.5
let logoScale: CGFloat = 1.18
let supersampling = 2
let cornerSteps = 128
let bitsPerComponent = 8
let bytesPerPixel = 4

enum IconError: Error {
    case arguments
    case source
    case context
    case output
}

func generate() throws {
    guard CommandLine.arguments.count == 3 else { throw IconError.arguments }
    let sourceURL = URL(fileURLWithPath: CommandLine.arguments[1])
    guard let source = CGImageSourceCreateWithURL(sourceURL as CFURL, nil),
          let image = CGImageSourceCreateImageAtIndex(source, 0, nil),
          image.width == image.height else { throw IconError.source }
    let pixels = canvasSize * supersampling
    let colorSpace = CGColorSpace(name: CGColorSpace.sRGB)
    guard let colorSpace,
          let context = CGContext(data: nil, width: pixels, height: pixels,
                                  bitsPerComponent: bitsPerComponent,
                                  bytesPerRow: pixels * bytesPerPixel, space: colorSpace,
                                  bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
    else { throw IconError.context }
    let scale = CGFloat(supersampling)
    context.scaleBy(x: scale, y: scale)
    let margin = (CGFloat(canvasSize) - artworkSize) / 2
    let radius = artworkSize * cornerRatio
    let lower = margin + radius
    let upper = margin + artworkSize - radius
    let centers = [CGPoint(x: upper, y: upper), CGPoint(x: lower, y: upper),
                   CGPoint(x: lower, y: lower), CGPoint(x: upper, y: lower)]
    let path = CGMutablePath()
    for (corner, center) in centers.enumerated() {
        for step in 0...cornerSteps {
            let angle = (CGFloat(corner) + CGFloat(step) / CGFloat(cornerSteps)) * .pi / 2
            let cosine = cos(angle)
            let sine = sin(angle)
            let x = center.x + radius * (cosine < 0 ? -1 : 1) * pow(abs(cosine), 2 / cornerExponent)
            let y = center.y + radius * (sine < 0 ? -1 : 1) * pow(abs(sine), 2 / cornerExponent)
            let point = CGPoint(x: x, y: y)
            if corner == 0 && step == 0 {
                path.move(to: point)
            } else {
                path.addLine(to: point)
            }
        }
    }
    path.closeSubpath()
    context.addPath(path)
    context.clip()
    context.interpolationQuality = .high
    let logoSize = artworkSize * logoScale
    let logoMargin = (CGFloat(canvasSize) - logoSize) / 2
    let logoRect = CGRect(x: logoMargin, y: logoMargin, width: logoSize, height: logoSize)
    context.draw(image, in: logoRect)
    let outputURL = URL(fileURLWithPath: CommandLine.arguments[2])
    guard let rendered = context.makeImage(),
          let destination = CGImageDestinationCreateWithURL(outputURL as CFURL,
                                                           UTType.png.identifier as CFString, 1, nil)
    else { throw IconError.output }
    CGImageDestinationAddImage(destination, rendered, nil)
    guard CGImageDestinationFinalize(destination) else { throw IconError.output }
}

do {
    try generate()
} catch {
    FileHandle.standardError.write(Data("Icon generation failed: \(error)\n".utf8))
    exit(EXIT_FAILURE)
}
