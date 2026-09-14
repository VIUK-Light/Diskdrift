// Shared formatting and small reusable views.

import SwiftUI

func formatBytes(_ bytes: UInt64) -> String {
    let value = Double(bytes)
    if value >= 1e12 { return String(format: "%.1f TB", value / 1e12) }
    if value >= 1e9 { return String(format: "%.1f GB", value / 1e9) }
    if value >= 1e6 { return String(format: "%.1f MB", value / 1e6) }
    if value >= 1e3 { return String(format: "%.1f KB", value / 1e3) }
    return "\(bytes) B"
}

func formatDelta(_ bytes: Int64) -> String {
    (bytes >= 0 ? "+" : "-") + formatBytes(UInt64(abs(bytes)))
}

func displayPath(_ path: String) -> String {
    let home = NSHomeDirectory()
    if path == home { return "~" }
    if path.hasPrefix(home + "/") {
        return "~" + path.dropFirst(home.count)
    }
    return path
}

func shortTime(_ local: String) -> String {
    guard local.count >= 16 else { return local }
    let start = local.index(local.startIndex, offsetBy: 11)
    let end = local.index(local.startIndex, offsetBy: 16)
    return String(local[start..<end])
}

func shortDate(_ local: String) -> String {
    String(local.prefix(10))
}

struct Card<Content: View>: View {
    let title: String
    @ViewBuilder let content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(title)
                .font(.headline)
                .foregroundStyle(.secondary)
            content
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            RoundedRectangle(cornerRadius: 12)
                .fill(Color(nsColor: .controlBackgroundColor))
        )
        .overlay(
            RoundedRectangle(cornerRadius: 12)
                .stroke(Color(nsColor: .separatorColor), lineWidth: 1)
        )
    }
}

struct DeltaLabel: View {
    let bytes: Int64
    var font: Font = .body

    var body: some View {
        Text(formatDelta(bytes))
            .font(font.monospacedDigit())
            .foregroundColor(bytes >= 0 ? .orange : .green)
    }
}

struct SizeBar: View {
    let label: String
    let detail: String
    let value: UInt64
    let maxValue: UInt64
    let indent: Int

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                Text(String(repeating: "  ", count: indent) + label)
                    .lineLimit(1)
                Spacer()
                Text(detail)
                    .foregroundStyle(.secondary)
                    .font(.callout.monospacedDigit())
            }
            GeometryReader { proxy in
                let fraction = maxValue > 0 ? Double(value) / Double(maxValue) : 0
                ZStack(alignment: .leading) {
                    RoundedRectangle(cornerRadius: 3)
                        .fill(Color(nsColor: .quaternaryLabelColor).opacity(0.4))
                    RoundedRectangle(cornerRadius: 3)
                        .fill(Color.accentColor.opacity(0.75))
                        .frame(width: proxy.size.width * fraction)
                }
            }
            .frame(height: 6)
        }
    }
}
