import SwiftUI

struct SystemView: View {
    @EnvironmentObject var model: AppModel

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                VStack(alignment: .leading, spacing: 4) {
                    Text("System Storage").font(.largeTitle.bold())
                    Text("Estimated. Snapshot blocks and purgeable space are macOS-managed.")
                        .foregroundStyle(.secondary)
                }

                if let system = model.system {
                    volumesCard(system)
                    managedCard(system)
                    cachesCard(system)
                    notesCard(system)
                } else if let message = model.systemMessage {
                    Text(message).foregroundStyle(.secondary)
                } else {
                    HStack(spacing: 10) {
                        ProgressView().controlSize(.small)
                        Text("Reading system volumes…").foregroundStyle(.secondary)
                    }
                }
            }
            .padding(24)
            .frame(maxWidth: 860, alignment: .leading)
        }
        .navigationTitle("System")
        .onAppear {
            if model.system == nil {
                model.loadSystem()
            }
        }
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button {
                    model.loadSystem()
                } label: {
                    Label("Refresh", systemImage: "arrow.clockwise")
                }
                .disabled(model.isBusy)
            }
        }
    }

    private func volumesCard(_ system: SystemResult) -> some View {
        Card(title: "Volumes") {
            ForEach(system.volumes) { volume in
                VStack(alignment: .leading, spacing: 6) {
                    HStack {
                        Text(volume.mountPoint)
                            .font(.headline)
                        if volume.isSystem {
                            Text("system")
                                .font(.caption2)
                                .padding(.horizontal, 5)
                                .padding(.vertical, 1)
                                .background(Capsule().fill(Color.secondary.opacity(0.15)))
                                .foregroundStyle(.secondary)
                        }
                        Spacer()
                        Text(
                            "\(formatBytes(volume.usedBytes)) / \(formatBytes(volume.totalBytes))"
                        )
                        .foregroundStyle(.secondary)
                        .font(.callout.monospacedDigit())
                    }
                    ProgressView(
                        value: Double(volume.usedBytes),
                        total: Double(max(volume.totalBytes, 1))
                    )
                    .progressViewStyle(.linear)
                    Text("\(volume.fsType) · \(volume.device) · \(formatBytes(volume.availableBytes)) available")
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
                .padding(.vertical, 6)
            }
        }
    }

    private func managedCard(_ system: SystemResult) -> some View {
        Card(title: "macOS managed") {
            HStack {
                Label("Local snapshots", systemImage: "clock.arrow.circlepath")
                Spacer()
                if let latest = system.snapshotLatest {
                    Text("\(system.snapshotCount) · latest \(latest)")
                        .foregroundStyle(.secondary)
                } else {
                    Text("none found").foregroundStyle(.secondary)
                }
            }
            HStack {
                Label("VM / Swap", systemImage: "memorychip")
                Spacer()
                if let vm = system.vm {
                    Text("\(formatBytes(vm.allocatedBytes)) · \(vm.label)")
                        .foregroundStyle(.secondary)
                } else {
                    Text("not readable").foregroundStyle(.secondary)
                }
            }
        }
    }

    private func cachesCard(_ system: SystemResult) -> some View {
        Card(title: "System caches") {
            if let caches = system.systemCaches {
                HStack {
                    Text(displayPath(caches.path))
                    Spacer()
                    Text(formatBytes(caches.allocatedBytes)).monospacedDigit()
                }
                Text(caches.label)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            } else {
                Text("not readable").foregroundStyle(.secondary)
            }
        }
    }

    private func notesCard(_ system: SystemResult) -> some View {
        Card(title: "Notes") {
            ForEach(Array(system.notes.enumerated()), id: \.offset) { _, note in
                Text("· \(note)")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
    }
}
