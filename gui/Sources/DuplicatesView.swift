import SwiftUI

struct DuplicatesView: View {
    @EnvironmentObject var model: AppModel
    @State private var minSizeMB = 10.0
    @State private var modelsOnly = false

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            VStack(alignment: .leading, spacing: 4) {
                Text("Duplicates").font(.largeTitle.bold())
                Text("Size → partial hash → full hash. Hard links are never counted.")
                    .foregroundStyle(.secondary)
            }

            HStack(spacing: 14) {
                Picker("Minimum size", selection: $minSizeMB) {
                    Text("1 MB").tag(1.0)
                    Text("10 MB").tag(10.0)
                    Text("100 MB").tag(100.0)
                    Text("1 GB").tag(1024.0)
                }
                .frame(maxWidth: 320)
                Toggle("Only AI models", isOn: $modelsOnly)
                Button {
                    model.findDuplicates(
                        minSize: UInt64(minSizeMB * 1_000_000), modelsOnly: modelsOnly)
                } label: {
                    Label("Find duplicates", systemImage: "magnifyingglass")
                }
                .disabled(model.duplicatesRunning)
                Spacer()
            }

            if model.duplicatesRunning {
                HStack(spacing: 10) {
                    ProgressView().controlSize(.small)
                    Text("Hashing candidates…").foregroundStyle(.secondary)
                }
            } else if let duplicates = model.duplicates {
                if duplicates.groups.isEmpty {
                    Text("No duplicates found above the minimum size.")
                        .foregroundStyle(.secondary)
                } else {
                    Text(
                        "\(duplicates.groups.count) groups · potentially reclaimable ~\(formatBytes(duplicates.reclaimableBytes))"
                    )
                    .font(.headline)
                    List(duplicates.groups) { group in
                        VStack(alignment: .leading, spacing: 6) {
                            HStack {
                                if let model = group.model {
                                    Text(
                                        model.quantization.map { "\(model.name) (\($0))" }
                                            ?? model.name
                                    )
                                    .font(.headline)
                                } else {
                                    Text("\(group.files.count) identical files")
                                        .font(.headline)
                                }
                                Spacer()
                                Text(formatBytes(group.allocatedBytes))
                                    .monospacedDigit()
                                    .foregroundStyle(.secondary)
                            }
                            ForEach(group.files) { file in
                                HStack {
                                    Text(displayPath(file.path))
                                        .font(.callout)
                                        .lineLimit(1)
                                        .truncationMode(.middle)
                                    if file.hardLink {
                                        Text("hard link")
                                            .font(.caption2)
                                            .foregroundStyle(.tertiary)
                                    }
                                    Spacer()
                                }
                            }
                            Text("Potential duplicate: \(formatBytes(group.reclaimableBytes))")
                                .font(.caption)
                                .foregroundStyle(.orange)
                        }
                        .padding(.vertical, 4)
                    }
                    .listStyle(.inset)
                }
            } else if let message = model.duplicatesMessage {
                Text(message).foregroundStyle(.secondary)
            }
        }
        .padding(24)
        .navigationTitle("Duplicates")
    }
}
