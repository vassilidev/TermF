// Reference QR codes from macOS's own encoder (CoreImage CIQRCodeGenerator, level M), one module per pixel, printed
// as rows of 0 and 1 without the quiet zone: `swift qr-reference.swift "text" ...`.
import CoreImage
import Foundation

let context = CIContext()
for text in CommandLine.arguments.dropFirst() {
    let filter = CIFilter(name: "CIQRCodeGenerator")!
    filter.setValue(text.data(using: .utf8), forKey: "inputMessage")
    filter.setValue("M", forKey: "inputCorrectionLevel")
    let image = filter.outputImage!
    let extent = image.extent
    let width = Int(extent.width), height = Int(extent.height)
    var pixels = [UInt8](repeating: 0, count: width * height * 4)
    context.render(image, toBitmap: &pixels, rowBytes: width * 4, bounds: extent, format: .RGBA8, colorSpace: CGColorSpaceCreateDeviceRGB())
    var rows: [String] = []
    for y in 0..<height {
        var row = ""
        for x in 0..<width { row += pixels[(y * width + x) * 4] < 128 ? "1" : "0" }
        rows.append(row)
    }
    // Trim the light border.
    let dark = rows.enumerated().filter { $0.element.contains("1") }.map { $0.offset }
    let top = dark.first!, bottom = dark.last!
    let cols = rows.flatMap { r in r.enumerated().filter { $0.element == "1" }.map { $0.offset } }
    let left = cols.min()!, right = cols.max()!
    print("# \(text)")
    for y in top...bottom {
        let r = rows[y]
        let s = r.index(r.startIndex, offsetBy: left), e = r.index(r.startIndex, offsetBy: right)
        print(String(r[s...e]))
    }
}
