import SwiftUI

@main
struct DiskDriftApp: App {
    @StateObject private var model = AppModel()
    @StateObject private var menu = MenuBarModel()

    init() {
        SettingsKeys.registerDefaults()
    }

    var body: some Scene {
        WindowGroup(id: "main") {
            ContentView()
                .environmentObject(model)
                .frame(minWidth: 980, minHeight: 640)
                .task {
                    model.refreshDashboard()
                    model.loadHistory(category: nil)
                    menu.start()
                }
        }
        .commands {
            CommandGroup(replacing: .newItem) {}
        }

        MenuBarExtra {
            MenuBarView()
                .environmentObject(menu)
        } label: {
            HStack(spacing: 4) {
                Image(systemName: "internaldrive")
                Text(menu.title)
            }
        }
        .menuBarExtraStyle(.window)

        Settings {
            SettingsView()
        }
    }
}
