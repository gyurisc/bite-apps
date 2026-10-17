//
//  TextBiteApp.swift
//  TextBite
//
//  Created by Pip Kadabra on 2026. 10. 13..
//

import SwiftUI

@main
struct TextBiteApp: App {
    /// The app's one engine; each window's document lives in it.
    @State private var engine = EngineBridge()

    var body: some Scene {
        WindowGroup {
            ContentView(engine: engine)
        }
    }
}
