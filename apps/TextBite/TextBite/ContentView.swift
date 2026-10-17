//
//  ContentView.swift
//  TextBite
//
//  Created by Pip Kadabra on 2026. 10. 13..
//

import SwiftUI

struct ContentView: View {
    @State private var model = DocumentModel()

    /// UI tests launch the app with this argument to see the engine's text.
    private let showsEngineText = ProcessInfo.processInfo.arguments.contains("--ui-testing")

    var body: some View {
        VStack(spacing: 0) {
            DocumentEditor(model: model)
            if let errorMessage = model.errorMessage {
                Text(errorMessage)
                    .foregroundStyle(.red)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(6)
            }
            if showsEngineText {
                Text(model.document?.text ?? "")
                    .accessibilityIdentifier("engine-text")
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(6)
            }
        }
        .frame(minWidth: 360, minHeight: 200)
    }
}

#Preview {
    ContentView()
}
