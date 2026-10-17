//
//  ContentView.swift
//  TextBite
//
//  Created by Pip Kadabra on 2026. 10. 13..
//

import SwiftUI

/// One window: an editor for the window's own document in the app's engine.
struct ContentView: View {
    let engine: EngineBridge
    @State private var model: DocumentModel
    /// A field outside the document, shown only to UI tests, which check
    /// that it keeps its own undo.
    @State private var scratch = ""

    /// UI tests launch the app with this argument to see the engine's text.
    private let showsEngineText = ProcessInfo.processInfo.arguments.contains("--ui-testing")

    init(engine: EngineBridge) {
        self.engine = engine
        // SwiftUI may make this view more than once for a window; a model
        // touches the engine only when the window appears.
        _model = State(initialValue: DocumentModel(engine: engine))
    }

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
                TextField("Scratch", text: $scratch)
                    .accessibilityIdentifier("scratch-field")
                    .padding(6)
                Text("\(engine.openDocuments.count) open")
                    .accessibilityIdentifier("open-documents")
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(6)
            }
        }
        .frame(minWidth: 360, minHeight: 200)
        // The editor closes the document when the window closes.
        .onAppear { model.open() }
    }
}

#Preview {
    ContentView(engine: EngineBridge())
}
