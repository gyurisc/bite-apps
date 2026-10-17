//
//  ContentView.swift
//  TextBite
//
//  Created by Pip Kadabra on 2026. 10. 13..
//

import SwiftUI

struct ContentView: View {
    @State private var model = DocumentModel()
    @State private var draft = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                TextField("Text to send to the engine", text: $draft)
                    .onSubmit(replaceText)
                    .accessibilityIdentifier("draft")
                Button("Replace Text", action: replaceText)
            }
            if let document = model.document {
                Text("Document \(document.id) in the engine")
                    .font(.headline)
                Text(document.text)
                    .textSelection(.enabled)
                    .accessibilityIdentifier("document-text")
                Text(document.canUndo ? "The engine can undo this." : "Nothing to undo yet.")
                    .foregroundStyle(.secondary)
                    .accessibilityIdentifier("undo-status")
            }
            if let errorMessage = model.errorMessage {
                Text(errorMessage)
                    .foregroundStyle(.red)
            }
        }
        .padding()
        .frame(minWidth: 360, minHeight: 160, alignment: .topLeading)
    }

    private func replaceText() {
        model.replaceText(with: draft)
    }
}

#Preview {
    ContentView()
}
