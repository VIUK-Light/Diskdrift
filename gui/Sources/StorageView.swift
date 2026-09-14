import SwiftUI

struct StorageView: View {
    @EnvironmentObject var model: AppModel
    @State private var selectedCategory: String?

    var body: some View {
        HSplitView {
            categoryPane
                .frame(minWidth: 340, idealWidth: 420)
            directoryPane
                .frame(minWidth: 380)
        }
        .navigationTitle("Storage")
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button {
                    model.runScan()
                } label: {
                    Label("Scan", systemImage: "arrow.clockwise")
                }
                .disabled(model.isBusy)
            }
        }
    }

    private var categoryPane: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                if let scan = model.scan {
                    Text("Scanned \(formatBytes(scan.totals.allocatedBytes))")
                        .font(.title3.bold())
                    Text(
                        "\(scan.totals.fileCount) files · \(scan.totals.directoryCount) directories"
                    )
                    .font(.caption)
                    .foregroundStyle(.secondary)

                    if selectedCategory != nil {
                        Button {
                            selectedCategory = nil
                        } label: {
                            Label("Show all categories", systemImage: "xmark.circle")
                        }
                        .buttonStyle(.link)
                        .font(.caption)
                    }

                    let categoryMap = Dictionary(
                        uniqueKeysWithValues: scan.categories.map { ($0.id, $0) })
                    let maxValue = scan.categories.map(\.allocatedBytes).max() ?? 1
                    ForEach(scan.categories) { category in
                        SizeBar(
                            label: category.name,
                            detail: formatBytes(category.allocatedBytes),
                            value: category.allocatedBytes,
                            maxValue: maxValue,
                            indent: categoryDepth(category, categoryMap)
                        )
                        .contentShape(Rectangle())
                        .onTapGesture {
                            selectedCategory =
                                selectedCategory == category.id ? nil : category.id
                        }
                        .opacity(
                            selectedCategory == nil || selectedCategory == category.id
                                ? 1 : 0.45)
                    }
                } else {
                    emptyState
                }
            }
            .padding(20)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private var directoryPane: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text(selectedCategoryTitle)
                    .font(.headline)
                Spacer()
                if let scan = model.scan {
                    Text("\(filteredDirectories(scan).count) directories")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            .padding(.horizontal, 20)
            .padding(.top, 20)

            if let scan = model.scan {
                List(filteredDirectories(scan).prefix(300)) { entry in
                    HStack(spacing: 10) {
                        Text(displayPath(entry.path))
                            .lineLimit(1)
                            .truncationMode(.middle)
                        Spacer()
                        Text(formatBytes(entry.allocatedBytes))
                            .font(.callout.monospacedDigit())
                            .foregroundStyle(.secondary)
                    }
                }
                .listStyle(.inset)
            } else {
                Spacer()
                emptyState
                Spacer()
            }
        }
    }

    private var emptyState: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("No scan yet.").font(.headline)
            Text("Run a scan to see categories and directories.")
                .foregroundStyle(.secondary)
            Button("Scan now") { model.runScan() }
                .disabled(model.isBusy)
        }
    }

    private var selectedCategoryTitle: String {
        guard let id = selectedCategory, let scan = model.scan else {
            return "Largest directories"
        }
        return scan.categories.first(where: { $0.id == id })?.name ?? "Largest directories"
    }

    private func filteredDirectories(_ scan: ScanResult) -> [DirectoryEntry] {
        guard let selected = selectedCategory else { return scan.directories }
        return scan.directories.filter { entry in
            entry.categoryId == selected || entry.categoryId.hasPrefix(selected + ".")
        }
    }

    private func categoryDepth(_ category: Category, _ map: [String: Category]) -> Int {
        var depth = 0
        var current = category
        while let parentId = current.parentId, let parent = map[parentId] {
            depth += 1
            current = parent
            if depth > 6 { break }
        }
        return depth
    }
}
