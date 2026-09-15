// DiskDrift core wrapper. The Rust core does all storage work; these types
// decode its versioned JSON (schema 1) — the GUI never reimplements logic.

import Foundation

struct CoreError: Error, LocalizedError {
    let message: String
    var errorDescription: String? { message }
}

// MARK: - Payloads

struct DiskUsage: Decodable {
    let path: String
    let totalBytes: UInt64
    let freeBytes: UInt64
    let availableBytes: UInt64
    let usedBytes: UInt64
}

struct Totals: Decodable {
    let logicalBytes: UInt64
    let allocatedBytes: UInt64
    let fileCount: UInt64
    let directoryCount: UInt64
}

struct Category: Decodable, Identifiable {
    let id: String
    let name: String
    let parentId: String?
    let logicalBytes: UInt64
    let allocatedBytes: UInt64
    let fileCount: UInt64
    let directoryCount: UInt64
}

struct DirectoryEntry: Decodable, Identifiable {
    let path: String
    let categoryId: String
    let logicalBytes: UInt64
    let allocatedBytes: UInt64
    let fileCount: UInt64
    let directoryCount: UInt64
    var id: String { path }
}

struct ScanResult: Decodable {
    let version: Int
    let timestamp: String
    let totals: Totals
    let categories: [Category]
    let directories: [DirectoryEntry]
}

struct SnapshotMeta: Decodable {
    let id: Int
    let createdAtLocal: String
    let totalLogicalBytes: UInt64
    let totalAllocatedBytes: UInt64
    let fileCount: UInt64
    let directoryCount: UInt64
    let skippedCount: UInt64
}

struct SnapshotResult: Decodable {
    let snapshot: SnapshotMeta
}

struct HistoryCategory: Decodable {
    let id: String
    let name: String
}

struct HistoryDay: Decodable, Identifiable {
    let date: String
    let snapshotId: Int
    let snapshotCount: Int
    let allocatedBytes: UInt64
    let logicalBytes: UInt64
    let changeBytes: Int64?
    var id: String { date }
}

struct HistoryResult: Decodable {
    let category: HistoryCategory?
    let days: [HistoryDay]
}

struct EventEntry: Decodable, Identifiable {
    let id: Int
    let timestamp: String
    let kind: String
    let path: String
    let categoryId: String
    let deltaBytes: Int64
    let allocatedBytes: UInt64
}

struct EventsResult: Decodable {
    let events: [EventEntry]
}

struct Incident: Decodable, Identifiable {
    let label: String
    let categoryId: String
    let path: String?
    let deltaBytes: Int64
    let growBytes: UInt64
    let shrinkBytes: UInt64
    let firstEvent: String
    let lastEvent: String
    let eventCount: Int
    var id: String { label + (path ?? "") + firstEvent }
}

struct WhatHappenedTotal: Decodable {
    let deltaBytes: Int64
    let growBytes: UInt64
    let shrinkBytes: UInt64
    let eventCount: Int
}

struct SystemVolume: Decodable, Identifiable {
    let mountPoint: String
    let device: String
    let fsType: String
    let totalBytes: UInt64
    let usedBytes: UInt64
    let freeBytes: UInt64
    let availableBytes: UInt64
    let isSystem: Bool
    var id: String { mountPoint }
}

struct SystemPathSize: Decodable {
    let path: String
    let allocatedBytes: UInt64
    let logicalBytes: UInt64
    let fileCount: UInt64
    let directoryCount: UInt64
    let label: String
}

struct SystemResult: Decodable {
    let volumes: [SystemVolume]
    let snapshotCount: Int
    let snapshotLatest: String?
    let vm: SystemPathSize?
    let systemCaches: SystemPathSize?
    let notes: [String]
}

struct WhatHappenedResult: Decodable {
    let window: String
    let from: String
    let to: String
    let total: WhatHappenedTotal
    let incidents: [Incident]
}

private struct ErrorPayload: Decodable {
    let error: String?
}

private struct VersionPayload: Decodable {
    let version: String
}

// MARK: - Core

enum Core {
    static func version() throws -> String {
        try decode(VersionPayload.self, from: call { dd_version() }).version
    }

    static func diskUsage(path: String) throws -> DiskUsage {
        try withCString(path) { cPath in
            try decode(DiskUsage.self, from: call { dd_disk_usage(cPath) })
        }
    }

    static func scan(
        home: String?, dataDir: String?, path: String?, depth: Int, threads: Int
    ) throws -> ScanResult {
        try withThree(home, dataDir, path) { cHome, cData, cPath in
            try decode(
                ScanResult.self,
                from: call { dd_scan(cHome, cData, cPath, Int32(depth), Int32(threads)) }
            )
        }
    }

    static func snapshot(
        home: String?, dataDir: String?, path: String?, depth: Int, threads: Int
    ) throws -> SnapshotResult {
        try withThree(home, dataDir, path) { cHome, cData, cPath in
            try decode(
                SnapshotResult.self,
                from: call { dd_snapshot(cHome, cData, cPath, Int32(depth), Int32(threads)) }
            )
        }
    }

    static func system(home: String?, threads: Int) throws -> SystemResult {
        try withCString(home) { cHome in
            try decode(SystemResult.self, from: call { dd_system(cHome, Int32(threads)) })
        }
    }

    static func history(home: String?, dataDir: String?, category: String?) throws -> HistoryResult {
        try withThree(home, dataDir, category) { cHome, cData, cCategory in
            try decode(HistoryResult.self, from: call { dd_history(cHome, cData, cCategory) })
        }
    }

    static func events(home: String?, dataDir: String?, since: String?, limit: Int) throws -> EventsResult {
        try withThree(home, dataDir, since) { cHome, cData, cSince in
            try decode(
                EventsResult.self,
                from: call { dd_events(cHome, cData, cSince, Int32(limit)) }
            )
        }
    }

    static func whatHappened(
        home: String?, dataDir: String?, since: String?, from: String?, to: String?, limit: Int
    ) throws -> WhatHappenedResult {
        try withCString(home) { cHome in
            try withCString(dataDir) { cData in
                try withCString(since) { cSince in
                    try withCString(from) { cFrom in
                        try withCString(to) { cTo in
                            try decode(
                                WhatHappenedResult.self,
                                from: call {
                                    dd_what_happened(
                                        cHome, cData, cSince, cFrom, cTo, Int32(limit)
                                    )
                                }
                            )
                        }
                    }
                }
            }
        }
    }

    // MARK: - Helpers

    private static func call(_ body: () -> UnsafeMutablePointer<CChar>?) throws -> Data {
        guard let pointer = body() else {
            throw CoreError(message: "DiskDrift core returned no data")
        }
        defer { dd_free_string(pointer) }
        return Data(String(cString: pointer).utf8)
    }

    private static func decode<T: Decodable>(_ type: T.Type, from data: Data) throws -> T {
        if let payload = try? JSONDecoder().decode(ErrorPayload.self, from: data),
           let message = payload.error {
            throw CoreError(message: message)
        }
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        do {
            return try decoder.decode(type, from: data)
        } catch {
            throw CoreError(message: "cannot decode core response: \(error)")
        }
    }

    private static func withCString<T>(
        _ value: String?, _ body: (UnsafePointer<CChar>?) throws -> T
    ) rethrows -> T {
        if let value {
            return try value.withCString { pointer in
                try body(pointer)
            }
        }
        return try body(nil)
    }

    private static func withThree<T>(
        _ first: String?,
        _ second: String?,
        _ third: String?,
        _ body: (UnsafePointer<CChar>?, UnsafePointer<CChar>?, UnsafePointer<CChar>?) throws -> T
    ) rethrows -> T {
        try withCString(first) { a in
            try withCString(second) { b in
                try withCString(third) { c in
                    try body(a, b, c)
                }
            }
        }
    }
}
