// Deterministic smoke test for the C ABI + Swift wrapper (no GUI).
import Foundation

func expect(_ condition: Bool, _ message: String) {
    if !condition {
        fputs("SMOKE FAIL: \(message)\n", stderr)
        exit(1)
    }
}

let version = try Core.version()
expect(!version.isEmpty, "version")

let usage = try Core.diskUsage(path: "/")
expect(usage.totalBytes > 0, "disk usage total")
expect(usage.usedBytes <= usage.totalBytes, "disk usage used")

let fm = FileManager.default
let dir = fm.temporaryDirectory
    .appendingPathComponent("diskdrift-smoke-\(UUID().uuidString)")
let dataDir = dir.appendingPathComponent("data")
try fm.createDirectory(at: dir.appendingPathComponent("sub"), withIntermediateDirectories: true)
try Data(count: 4_000).write(to: dir.appendingPathComponent("sub/a.bin"))

let scan = try Core.scan(
    home: dir.path, dataDir: dataDir.path, path: dir.path, depth: 2, threads: 2)
expect(scan.totals.logicalBytes == 4_000, "scan totals")

let snapshot = try Core.snapshot(
    home: dir.path, dataDir: dataDir.path, path: dir.path, depth: 2, threads: 2)
expect(snapshot.snapshot.id == 1, "snapshot id")

let history = try Core.history(home: dir.path, dataDir: dataDir.path, category: nil)
expect(history.days.count == 1, "history days")

let what = try Core.whatHappened(
    home: dir.path, dataDir: dataDir.path, since: "24h", from: nil, to: nil, limit: 10)
expect(what.total.eventCount == 0, "what happened empty")

do {
    _ = try Core.scan(
        home: dir.path, dataDir: nil, path: "/definitely/not/here", depth: 0, threads: 0)
    expect(false, "missing path should fail")
} catch {
    // expected
}

try? fm.removeItem(at: dir)
print("SMOKE OK (core \(version))")
