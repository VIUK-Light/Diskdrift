import SwiftUI

@main
struct DiskDriftApp: App {
    @StateObject private var model = AppModel()

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environmentObject(model)
                .frame(minWidth: 980, minHeight: 640)
                .task {
                    model.refreshDashboard()
                    model.loadHistory(category: nil)
                }
        }
        .commands {
            CommandGroup(replacing: .newItem) {}
        }
    }
}
